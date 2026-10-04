//! Falhas de tool devolvidas **ao modelo** (não abortam o turno): argumentos inválidos e
//! truncagem (`finish_reason=length`, L-Q2).
//!
//! O modelo vê o erro e corrige no passo seguinte; só um nome fora do catálogo é fail-closed (é
//! um erro do turno, não um argumento corrigível). A ordem §42 mantém-se: `ToolCall` logado antes
//! do efeito, `ToolResult` a fechar.

use katu_core::error::ToolOutcome;
use katu_core::kernel::CallId;
use katu_policy::{ControlId, ToolArgs};

use super::{AgentError, CallOutcome, router};
use crate::runtime::Runtime;

/// Converte um erro de roteamento num resultado de tool **devolvido ao modelo**.
///
/// # Errors
/// [`AgentError::Route`] se o nome da tool estiver fora do catálogo (fail-closed).
pub(crate) fn route_failure(
    runtime: &mut Runtime<'_>,
    call: CallId,
    name: &str,
    error: &router::RouteError,
    now_millis: u64,
) -> Result<CallOutcome, AgentError> {
    let _span = katu_core::trace_fn!("agent::failure::route_failure");

    let Some(tool_name) = router::tool_name_for(name) else {
        return Err(AgentError::Route(router::RouteError::UnknownTool(
            name.to_string(),
        )));
    };
    let use_ = router::use_of(tool_name, ToolArgs::Other, Vec::new(), None, &runtime.cwd);
    runtime
        .session
        .begin_call(call.clone(), &use_, now_millis)?;
    let outcome = ToolOutcome::Unavailable {
        control: ControlId::new("argument"),
        rule_id: None,
    };
    runtime.session.settle_call(
        call,
        outcome.clone(),
        Some(format!("argumento inválido em `{name}`: {error}")),
    )?;
    Ok(CallOutcome {
        outcome,
        use_: None,
        approval: None,
        delta: None,
        terminate: false,
    })
}

/// Fecha uma tool call **truncada** (`finish_reason=length`) sem a executar (L-Q2).
///
/// O modelo recebe `Unavailable{length}` e um delta que **ensina** (reformular a chamada ou aumentar
/// `--max-tokens`); o turno **não** aborta. A ordem §42 mantém-se: o `ToolCall` já está no log
/// (veio do provider) e é fechado por um `ToolResult`.
///
/// # Errors
/// [`AgentError::Route`] se o nome da tool estiver fora do catálogo; [`AgentError::Session`] se o
/// log recusar.
pub(crate) fn settle_truncated(
    runtime: &mut Runtime<'_>,
    truncated: Vec<(CallId, String)>,
) -> Result<(), AgentError> {
    let _span = katu_core::trace_fn!("agent::failure::settle_truncated");

    let now = runtime.clock.now().as_millis();
    for (call, name) in truncated {
        let Some(tool_name) = router::tool_name_for(&name) else {
            return Err(AgentError::Route(router::RouteError::UnknownTool(name)));
        };
        let use_ = router::use_of(tool_name, ToolArgs::Other, Vec::new(), None, &runtime.cwd);
        runtime.session.begin_call(call.clone(), &use_, now)?;
        let outcome = ToolOutcome::Unavailable {
            control: ControlId::new("length"),
            rule_id: None,
        };
        runtime.session.settle_call(
            call,
            outcome,
            Some(format!(
                "a resposta foi truncada (`finish_reason=length`) a meio da tool `{name}`; \
                 reformula a chamada com os argumentos completos ou aumenta `--max-tokens`"
            )),
        )?;
    }
    Ok(())
}
