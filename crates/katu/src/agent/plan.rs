//! Execução da tool `plan` (E09-T04): regista o plano validado no kernel pela ordem §42.
//!
//! O plano vem do artefacto carregado no arranque (`Runtime::plan`), **não** dos argumentos do
//! modelo (que só tem `goal`/`next_action`). A validação é do [`PlanTool`]; só um plano válido é
//! logado como `PlanRecorded`, tornando a fase `Planned` alcançável. Sem artefacto, o roteador
//! devolve `Unavailable{scope-contract}` e nada é executado.

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Event, SessionError, dispatch};
use katu_core::plan::Plan;
use katu_policy::ToolUse;
use katu_tools::plan::PlanTool;

use super::AgentError;
use crate::runtime::Runtime;

/// Executa a validação/registo de um plano pela ordem §42 (logar → política → efeito).
///
/// # Errors
/// [`AgentError`] se a chamada, a política ou a transição falharem.
pub(super) fn execute(
    runtime: &mut Runtime<'_>,
    call: CallId,
    use_: &ToolUse,
    plan: &Plan,
) -> Result<ToolOutcome, AgentError> {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_PLAN, "plan::execute");
    runtime.session.apply(&Event::ToolCall {
        call: call.clone(),
        tool: use_.clone(),
    })?;
    let tool = PlanTool { plan: plan.clone() };
    let now = runtime.clock.now().as_millis();
    let outcome = dispatch(runtime.session.state(), use_, &runtime.rules, now, &tool)
        .map_err(SessionError::Policy)?
        .outcome();
    if outcome == ToolOutcome::Ok {
        runtime
            .session
            .apply(&Event::PlanRecorded { plan: plan.clone() })?;
    }
    runtime.session.apply(&Event::ToolResult {
        call,
        outcome: outcome.clone(),
        // O plano é um evento de controlo (`PlanRecorded`); não há envelope a devolver.
        delta: None,
    })?;
    Ok(outcome)
}
