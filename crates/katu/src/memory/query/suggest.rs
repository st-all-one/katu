//! Sugestões semânticas (`memo ask --suggest`, E20-T06).
//!
//! Degrada para vazio com aviso quando o índice vetorial está ausente/inválido (R33).

use katu_core::memory::{MemoryError, NoteRef, QueryOutcome, QueryReq, QueryResult, Suggestion};
use knudge_core::embeddings::{
    EmbeddingIndex, EmbeddingMeta, SuggestionPolicy, semantic_suggestions,
};

use super::super::translate::points;
use super::super::{Inner, to_memory_error};
use super::convert::anchors_of;

/// Modo `--suggest` (sugestões semânticas de aresta/contradição).
pub(super) fn suggest(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::suggest::suggest");

    let mut warnings = Vec::new();
    let meta = EmbeddingMeta::from_config(inner.kd.config()).map_err(to_memory_error)?;
    let loaded = EmbeddingIndex::load(
        inner.kd.fs_dyn(),
        &inner.kd.knowledge_dir(),
        &meta,
        &mut warnings,
    )
    .map_err(to_memory_error)?;
    let Some(embedding) = loaded else {
        warnings.push("índice vetorial indisponível: --suggest vazio".to_string());
        return Ok(QueryResult {
            outcome: QueryOutcome::Suggestions(Vec::new()),
            warnings,
        });
    };
    inner.ensure_index()?;
    inner.ensure_graph()?;
    let (Some(index), Some(graph)) = (inner.index.as_ref(), inner.graph.as_ref()) else {
        return Err(MemoryError::internal(
            "índice/grafo ausentes após construção",
        ));
    };
    let anchors_of = anchors_of(index);
    let policy = SuggestionPolicy {
        duplicate: 0.92,
        low: 0.4,
        high: 0.75,
    };
    let mut found = semantic_suggestions(&embedding, graph, &anchors_of, &policy, req.top_k);
    if let Some(relation) = req.relation.as_deref() {
        found.retain(|suggestion| suggestion.relation.as_str() == relation);
    }
    if req.limit > 0 {
        found.truncate(req.limit);
    }
    let suggestions = found
        .iter()
        .map(|suggestion| {
            Ok(Suggestion {
                relation: suggestion.relation.as_str().to_string(),
                from: NoteRef::new(suggestion.from.clone()),
                to: NoteRef::new(suggestion.to.clone()),
                score: points(suggestion.score)?,
            })
        })
        .collect::<Result<Vec<_>, MemoryError>>()?;
    Ok(QueryResult {
        outcome: QueryOutcome::Suggestions(suggestions),
        warnings,
    })
}
