//! Comando `sessions` (E03/ADR 0008): lista as sessões do projeto para retomar com `--resume`.
//!
//! Vive num módulo filho para manter `cli.rs` sob o teto de linhas. Só **lê** o índice temporal
//! (`<root>/.katu/sessions/index.jsonl`); nunca cria sessões nem toca no log.

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::{Session, discover_root};
use serde_json::{Value, json};

use crate::ports::StdFs;
use crate::report::Report;

/// Lista as sessões do projeto (id, instante, objetivo) em ordem temporal.
pub(super) fn list() -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_SESSIONS, "sessions::list");
    let fs = StdFs;
    let start = std::env::current_dir().unwrap_or_default();
    let root = discover_root(&fs, &start);
    match Session::list(&fs, &root) {
        Ok(metas) => {
            let listed: Vec<Value> = metas
                .iter()
                .map(|meta| {
                    json!({
                        "id": meta.id.as_str(),
                        "created_ms": meta.created_ms,
                        "goal": meta.goal,
                        "root": meta.root,
                    })
                })
                .collect();
            Report::ok(
                "sessions",
                Some(json!({
                    "root": root.display().to_string(),
                    "sessions": listed,
                })),
            )
        }
        Err(error) => Report::failed("sessions", &Error::internal(error.to_string())),
    }
}
