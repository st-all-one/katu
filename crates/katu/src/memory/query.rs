//! Consulta rica à memória (E20-T06): tradução katu → `knudge-core`.
//!
//! Vive em `crate::memory::*` (firewall `check-layers`): é o único sítio que fala com o knudge.
//! Espelha `kd ask`/`kd map` (filtros, `--rank`/`--tags`/`--suggest`/`--id`/`--around`, mapa
//! estrutural). Os modos `map`/`suggest` vivem em módulos filhos.

mod convert;
mod map;
mod suggest;

use katu_core::diag::{Level, events};
use katu_core::memory::{
    MemoryError, MemoryErrorKind, QueryMode, QueryOutcome, QueryReq, QueryResult, TagCount,
};
use knudge_core::lifecycle::DriftIndex;
use knudge_core::retrieval::{RankQuery, RecallQuery, get, rank, recall, tag_counts};
use knudge_core::schema::EdgeKind;

use super::{Inner, to_memory_error};
use convert::{anchors, convert_hits, filter, meta_map, note_hit, universe};

/// Executa a consulta rica no backend aberto.
pub(super) fn run(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::fn_span!(Level::Trace, events::MEMORY_RECALL, "query::run");
    match req.mode {
        QueryMode::Tags => tags(inner, req),
        QueryMode::Map => map::map(inner, req),
        QueryMode::Suggest => suggest::suggest(inner, req),
        QueryMode::Get => get_ids(inner, req),
        QueryMode::Expand => expand(inner, req),
        QueryMode::Rank => ranked(inner, req),
        _ => recall_text(inner, req),
    }
}

/// Modo `recall` textual (filtros + universo + escopo).
fn recall_text(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::recall_text");

    inner.ensure_index()?;
    inner.ensure_graph()?;
    let (Some(index), Some(graph)) = (inner.index.as_ref(), inner.graph.as_ref()) else {
        return Err(MemoryError::internal(
            "índice/grafo ausentes após construção",
        ));
    };
    let mut query = RecallQuery::new(req.query.clone());
    query.limit = req.limit;
    query.filter = filter(req)?;
    query.universe = universe(req);
    query.scope.clone_from(&req.filter.scope);
    query.working_paths = anchors(req);
    query.now_ms = Some(inner.kd.now_ms());
    query.strict = inner.strict;
    let out = recall(index, graph, &query).map_err(to_memory_error)?;
    let metas = meta_map(index);
    Ok(QueryResult {
        outcome: QueryOutcome::Hits(convert_hits(&out.hits, &metas)?),
        warnings: out.warnings,
    })
}

/// Modo `rank` (notas mais confiáveis, sem query).
fn ranked(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::ranked");

    inner.ensure_index()?;
    inner.ensure_graph()?;
    let (Some(index), Some(graph)) = (inner.index.as_ref(), inner.graph.as_ref()) else {
        return Err(MemoryError::internal(
            "índice/grafo ausentes após construção",
        ));
    };
    let hits = rank(
        index,
        graph,
        &filter(req)?,
        &RankQuery {
            universe: universe(req),
            now_ms: Some(inner.kd.now_ms()),
            limit: req.limit,
            task_weight: 0.0,
            drift: DriftIndex::default(),
        },
    );
    let metas = meta_map(index);
    Ok(QueryResult {
        outcome: QueryOutcome::Hits(convert_hits(&hits, &metas)?),
        warnings: Vec::new(),
    })
}

/// Modo `--tags` (vocabulário).
fn tags(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::tags");

    inner.ensure_index()?;
    let Some(index) = inner.index.as_ref() else {
        return Err(MemoryError::internal("índice ausente após construção"));
    };
    let mut counts = tag_counts(index);
    if req.limit > 0 {
        counts.truncate(req.limit);
    }
    let tags = counts
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    Ok(QueryResult {
        outcome: QueryOutcome::Tags(tags),
        warnings: Vec::new(),
    })
}

/// Modo `--id` (recupera corpos por id).
fn get_ids(inner: &Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::get_ids");

    let store = inner.kd.store();
    let out = get(&store, &req.ids).map_err(to_memory_error)?;
    let hits = out
        .notes
        .iter()
        .map(|note| note_hit(note, req.full_content))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(QueryResult {
        outcome: QueryOutcome::Hits(hits),
        warnings: out.warnings,
    })
}

/// Modo `--around` (expande o grafo a partir de um id).
fn expand(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::expand");

    inner.ensure_graph()?;
    let Some(around) = req.around.as_deref() else {
        return Err(MemoryError::new(
            MemoryErrorKind::Invalid,
            "modo expand exige `--around`",
        ));
    };
    let Some(graph) = inner.graph.as_ref() else {
        return Err(MemoryError::internal("grafo ausente após construção"));
    };
    if !graph.contains(around) {
        return Err(MemoryError::new(
            MemoryErrorKind::Invalid,
            format!("nota ausente: {around}"),
        ));
    }
    let kind = match req.via.as_deref() {
        Some(via) => Some(via.parse::<EdgeKind>().map_err(to_memory_error)?),
        None => None,
    };
    let store = inner.kd.store();
    let mut ids = vec![around.to_string()];
    for hit in graph.expand(around, kind, u32::from(req.depth)) {
        ids.push(hit.id);
    }
    let hits = ids
        .iter()
        .map(|id| {
            store
                .read(id)
                .map(|note| note_hit(&note, req.full_content))
                .map_err(to_memory_error)?
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(QueryResult {
        outcome: QueryOutcome::Hits(hits),
        warnings: Vec::new(),
    })
}
