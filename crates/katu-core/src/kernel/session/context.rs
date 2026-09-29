//! Contexto de execução de uma tool call e registo do resultado.

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::kernel::pipeline::Tool;
use katu_policy::RuleSet;

/// Contexto de execução de uma tool call (evita uma assinatura com demasiados argumentos).
#[derive(Clone, Copy)]
pub struct CallContext<'a> {
    /// Regras de política a avaliar.
    pub rules: &'a RuleSet,
    /// Instante corrente (ms desde a época), injetado pelo clock do kernel.
    pub now_millis: u64,
    /// Tool a executar se a política permitir.
    pub tool: &'a dyn Tool,
}

/// Emite o evento estruturado de resultado (`tool.ok`/`tool.error`).
pub(super) fn log_outcome(outcome: &ToolOutcome) {
    if outcome.is_success() {
        crate::event!(Level::Debug, events::TOOL_OK);
    } else {
        crate::event!(Level::Warn, events::TOOL_ERROR);
    }
}
