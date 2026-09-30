//! Superfície CLI da memória (E03-T03/T07): `memory`, `recall`, `remember`.
//!
//! Vive junto do adaptador para que o binário fale com o `knudge-core` **só** em
//! `crate::memory::*` (firewall `check-layers`). Sem o adaptador, os comandos recusam (fail-closed).

use serde_json::{Value, json};

use super::KnudgeMemory;
use crate::report::Report;
use crate::runtime::RuntimeError;
use katu_core::error::Error;
use katu_core::memory::{Anchor, Memory, NoteRef, QueryHit, QueryOutcome, QueryReq, QueryResult};

/// Estado do backend de memória (adaptador in-process do knudge, E03-T07).
pub(crate) fn memory_status() -> Value {
    let root = std::env::current_dir().unwrap_or_default();
    match KnudgeMemory::open(&root) {
        Ok(memory) => match memory.status() {
            Ok(status) => json!({
                "backend": status.backend,
                "health": format!("{:?}", status.health),
                "warnings": status.warnings,
                "knowledge_dir": memory.knowledge_dir().display().to_string(),
            }),
            Err(error) => json!({ "error": error.to_string() }),
        },
        Err(error) => json!({ "error": error.to_string() }),
    }
}

/// Comando `memo drain --digest`: drena a fila de embeddings pelo pipeline do `knudge-core`
/// (E20-T20). Fail-closed: sem provedor ligado, devolve `enabled=false` sem tocar no índice.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`force` é o modo `--force` do dreno"
)]
pub(crate) fn memory_drain(force: bool) -> Report {
    let root = std::env::current_dir().unwrap_or_default();
    match KnudgeMemory::open(&root) {
        Ok(memory) => match memory.drain(force) {
            Ok(summary) => Report::ok(
                "memo.drain",
                Some(json!({
                    "enabled": summary.enabled,
                    "batches": summary.batches,
                    "indexed": summary.indexed,
                    "cache_hits": summary.cache_hits,
                    "rebuilt": summary.rebuilt,
                    "removed": summary.removed,
                    "warnings": summary.warnings,
                })),
            ),
            Err(error) => runtime_failure("memo.drain", RuntimeError::Memory(error)),
        },
        Err(error) => runtime_failure("memo.drain", RuntimeError::Memory(error)),
    }
}

/// Comando `memo ask`/`memo knowledge`: consulta rica pelo adaptador (E20-T06).
///
/// Read-only (não escreve memória); sem o adaptador, recusa (fail-closed).
pub(crate) fn memory_query(command: &'static str, req: &QueryReq) -> Report {
    let root = std::env::current_dir().unwrap_or_default();
    match KnudgeMemory::open(&root) {
        Ok(memory) => match memory.query(req) {
            Ok(result) => Report::ok(command, Some(query_value(&result))),
            Err(error) => runtime_failure(command, RuntimeError::Memory(error)),
        },
        Err(error) => runtime_failure(command, RuntimeError::Memory(error)),
    }
}

/// Envelope de uma consulta rica (forma + avisos).
fn query_value(result: &QueryResult) -> Value {
    let outcome = match &result.outcome {
        QueryOutcome::Hits(hits) => json!({
            "kind": "hits",
            "hits": hits.iter().map(hit_value).collect::<Vec<_>>(),
        }),
        QueryOutcome::Tags(tags) => json!({
            "kind": "tags",
            "tags": tags.iter().map(|tag| json!({"tag": tag.tag, "count": tag.count})).collect::<Vec<_>>(),
        }),
        QueryOutcome::Suggestions(items) => json!({
            "kind": "suggestions",
            "suggestions": items.iter().map(|item| json!({
                "relation": item.relation,
                "from": item.from.as_str(),
                "to": item.to.as_str(),
                "score": item.score.as_basis_points(),
            })).collect::<Vec<_>>(),
        }),
        QueryOutcome::Clusters(clusters) => json!({
            "kind": "clusters",
            "clusters": clusters.iter().map(|cluster| json!({
                "axis": cluster.axis,
                "key": cluster.key,
                "members": cluster.members.iter().map(NoteRef::as_str).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
        _ => json!({ "kind": "unknown" }),
    };
    json!({ "outcome": outcome, "warnings": result.warnings })
}

/// Um hit como JSON.
fn hit_value(hit: &QueryHit) -> Value {
    json!({
        "note": hit.note.as_str(),
        "statement": hit.statement,
        "score": hit.score.as_basis_points(),
        "basis": hit.basis.as_str(),
        "anchor": hit.anchor.as_ref().map(Anchor::as_str),
        "body": hit.body,
    })
}

/// Converte a falha do runtime na taxonomia estável de erro do katu.
fn runtime_failure(command: &'static str, error: RuntimeError) -> Report {
    let core: Error = error.into();
    Report::failed(command, &core)
}
