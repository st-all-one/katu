//! Tradução do `Dispatch` de um `!<cmd>` em eventos (extraído de `kernel.rs`).

use katu_core::api::Event as Update;
use katu_core::error::ToolOutcome;
use katu_core::kernel::Dispatch;
use katu_core::report::ToolReport;

/// Traduz o `Dispatch` de um `!<cmd>` em atualizações (recusa com evidência ou saída).
pub(crate) fn shell_updates(dispatch: &Dispatch) -> Vec<Update> {
    let _span = katu_core::trace_fn!("tui::kernel::shell_updates");

    match dispatch.outcome() {
        ToolOutcome::Denied { rule_id, evidence } => vec![Update::Error(format!(
            "negado por {}: {}",
            rule_id.as_str(),
            evidence.argument
        ))],
        ToolOutcome::Unavailable { control, .. } => {
            vec![Update::Error(format!(
                "! indisponível: {}",
                control.as_str()
            ))]
        }
        _ => {
            let body = dispatch
                .report()
                .map(ToolReport::to_toon)
                .unwrap_or_default();
            vec![Update::Info(body)]
        }
    }
}
