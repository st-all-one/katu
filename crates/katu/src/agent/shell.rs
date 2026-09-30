//! Execução de `!<cmd>` pela política/contenção (E20-T12).
//!
//! Constrói um `ToolUse` de `exec` (`sh -c <cmd>`) e passa-o pelo pipeline (§42) com as regras do
//! runtime. No modo plano, a regra `plan-no-shell` nega com evidência; fora dele, corre como a tool
//! `exec` (soft containment).

use katu_core::kernel::{CallContext, CallId, Dispatch};
use serde_json::json;

use super::router;
use super::{AgentError, Ports};
use crate::runtime::Runtime;

/// Despacha `sh -c <command>` pelo pipeline e devolve o `Dispatch` (decisão + efeito).
///
/// # Errors
/// [`AgentError`] se o roteamento ou a sessão falharem (fail-closed).
pub(crate) fn dispatch(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    command: &str,
) -> Result<Dispatch, AgentError> {
    let args = json!({ "argv": ["sh", "-c", command] });
    let root = runtime.root().to_path_buf();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: runtime.clock,
        root: &root,
    };
    let now = runtime.clock.now().as_millis();
    match router::route(&route_ports, &runtime.cwd, "exec", &args, None)? {
        router::Routed::Plain { use_, tool } => {
            let dispatch = runtime.session.tool_call(
                CallId::new("shell"),
                &use_,
                CallContext {
                    rules: &runtime.rules,
                    now_millis: now,
                    tool: tool.as_ref(),
                },
            )?;
            Ok(dispatch)
        }
        _ => Err(AgentError::Route(router::RouteError::UnknownTool(
            "exec".to_string(),
        ))),
    }
}
