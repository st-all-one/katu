//! Superfície CLI da memória (E03-T03/T07): `memory`, `recall`, `remember`.
//!
//! Vive junto do adaptador para que o binário fale com o `knudge-core` **só** em
//! `crate::memory::*` (firewall `check-layers`). Sem o adaptador, os comandos recusam (fail-closed).

use serde_json::{Value, json};

use super::KnudgeMemory;
use crate::ports::{StdFs, SystemClock};
use crate::report::Report;
use crate::runtime::{Runtime, RuntimeError};
use katu_core::error::Error;
use katu_core::kernel::Dispatch;
use katu_core::memory::Memory;

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

/// Comando `memo ask`: monta o runtime e consulta a memória pelo caminho §42.
pub(crate) fn memory_recall(query: &str, limit: usize) -> Report {
    let fs = StdFs;
    let clock = SystemClock;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match Runtime::open(&fs, &clock, &start, "cli: recall") {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failure("recall", error),
    };
    let project = runtime.root().display().to_string();
    match runtime.recall(query, limit) {
        Ok(dispatch) => Report::ok("memo.ask", Some(dispatch_value(&project, &dispatch))),
        Err(error) => runtime_failure("recall", error),
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

/// Converte a falha do runtime na taxonomia estável de erro do katu.
fn runtime_failure(command: &'static str, error: RuntimeError) -> Report {
    let core: Error = error.into();
    Report::failed(command, &core)
}

/// Envelope de dados de um `Dispatch` (resultado + relatório TOON projetado para JSON).
fn dispatch_value(project: &str, dispatch: &Dispatch) -> Value {
    let report = dispatch
        .report()
        .and_then(|report| report.to_json().ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    json!({
        "project": project,
        "ran": dispatch.ran(),
        "outcome": format!("{:?}", dispatch.outcome()),
        "report": report,
    })
}
