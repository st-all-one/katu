//! Conversão de tipos katu ↔ knudge para a consulta rica (E20-T06).

use std::collections::BTreeMap;

use katu_core::memory::{
    Anchor, Basis, MemoryError, NoteRef, QueryHit, QueryReq, QueryUniverse, Score,
    Status as KatuStatus,
};
use knudge_core::retrieval::{Filter, Index, Meta, RecallHit as KdHit, Universe};
use knudge_core::schema::{Classification, Status as KdStatus};
use knudge_core::store::Note;

use super::super::to_memory_error;
use super::super::translate::{from_note_type, from_status, note_type, points};

/// Filtro estrutural (statuses vazios = visíveis por omissão, como o `kd`).
pub(super) fn filter(req: &QueryReq) -> Result<Filter, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::convert::filter");

    let classifications = req
        .filter
        .classes
        .iter()
        .map(|raw| raw.parse::<Classification>().map_err(to_memory_error))
        .collect::<Result<Vec<_>, _>>()?;
    let statuses = if req.filter.statuses.is_empty() {
        KdStatus::VISIBLE.to_vec()
    } else {
        req.filter
            .statuses
            .iter()
            .map(|status| knudge_status(*status))
            .collect()
    };
    Ok(Filter {
        types: req.filter.types.iter().copied().map(note_type).collect(),
        classifications,
        statuses,
        tags: req.filter.tags.clone(),
        anchors: anchors(req),
    })
}

/// Âncoras como `String` (canal de âncoras + filtro).
pub(super) fn anchors(req: &QueryReq) -> Vec<String> {
    let _span = katu_core::trace_fn!("memory::query::convert::anchors");

    req.filter
        .anchors
        .iter()
        .map(|anchor| anchor.as_str().to_string())
        .collect()
}

/// Universo da consulta.
pub(super) fn universe(req: &QueryReq) -> Universe {
    let _span = katu_core::trace_fn!("memory::query::convert::universe");

    match req.universe {
        QueryUniverse::All => Universe::All,
        _ => Universe::Knowledge,
    }
}

/// Converte o estado do katu no do knudge.
fn knudge_status(status: KatuStatus) -> KdStatus {
    let _span = katu_core::trace_fn!("memory::query::convert::knudge_status");

    match status {
        KatuStatus::InProgress => KdStatus::InProgress,
        KatuStatus::Blocked => KdStatus::Blocked,
        KatuStatus::Closed => KdStatus::Closed,
        KatuStatus::Superseded => KdStatus::Superseded,
        KatuStatus::Forgotten => KdStatus::Forgotten,
        // `Active` e variantes futuras caem no default.
        _ => KdStatus::Active,
    }
}

/// Metadados por id (para enriquecer os hits).
pub(super) fn meta_map(index: &Index) -> BTreeMap<&str, &Meta> {
    let _span = katu_core::trace_fn!("memory::query::convert::meta_map");

    index
        .docs
        .iter()
        .map(|doc| (doc.meta.id.as_str(), &doc.meta))
        .collect()
}

/// Âncoras declaradas por id (para o `--suggest`).
pub(super) fn anchors_of(index: &Index) -> BTreeMap<String, Vec<String>> {
    let _span = katu_core::trace_fn!("memory::query::convert::anchors_of");

    index
        .docs
        .iter()
        .map(|doc| (doc.meta.id.clone(), doc.meta.anchors.clone()))
        .collect()
}

/// Converte hits do `recall`/`rank` em hits da porta.
pub(super) fn convert_hits(
    hits: &[KdHit],
    metas: &BTreeMap<&str, &Meta>,
) -> Result<Vec<QueryHit>, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::convert::convert_hits");

    hits.iter()
        .map(|hit| {
            let meta = metas.get(hit.id.as_str()).copied();
            Ok(QueryHit {
                note: NoteRef::new(hit.id.clone()),
                statement: hit.statement.clone(),
                score: points(hit.confidence)?,
                basis: Basis::Inferred,
                anchor: meta
                    .and_then(|meta| meta.anchors.first())
                    .map(|anchor| Anchor::new(anchor.clone())),
                note_type: meta.map(|meta| from_note_type(meta.note_type)),
                status: meta.map(|meta| from_status(meta.status)),
                tags: meta.map(|meta| meta.tags.clone()).unwrap_or_default(),
                body: None,
            })
        })
        .collect()
}

/// Converte uma nota do store num hit (com corpo opcional).
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`full` é o modo `--full-content`"
)]
pub(super) fn note_hit(note: &Note, full: bool) -> Result<QueryHit, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::convert::note_hit");

    let frontmatter = &note.frontmatter;
    let score = Score::from_basis_points(Score::MAX_BASIS_POINTS)
        .ok_or_else(|| MemoryError::internal("score máximo inválido"))?;
    Ok(QueryHit {
        note: NoteRef::new(frontmatter.id().map_err(to_memory_error)?.to_string()),
        statement: frontmatter
            .statement()
            .map_err(to_memory_error)?
            .to_string(),
        score,
        basis: Basis::Measured,
        anchor: None,
        note_type: Some(from_note_type(
            frontmatter.note_type().map_err(to_memory_error)?,
        )),
        status: Some(from_status(frontmatter.status().map_err(to_memory_error)?)),
        tags: Vec::new(),
        body: full.then(|| note.body.clone()),
    })
}
