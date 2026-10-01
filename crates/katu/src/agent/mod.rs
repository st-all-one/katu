//! Loop de turnos do agente (E12-T05/E10): provider ↔ kernel, tool execution pela ordem §42.
//!
//! O provider só **fala** com o endpoint de modelo; o kernel é o dono do loop. Cada pedido do
//! modelo é logado **antes** de executar (`ToolCall`), a política decide e só então o executor
//! corre (`ToolResult`) — `Session::tool_call` garante a ordem. O histórico enviado ao modelo é a
//! projeção do log (`derive_messages`), pelo que `Model-visible ⟺ logged` se mantém (E04).
//!
//! Fail-closed: uma tool fora do catálogo, um argumento em falta ou um caminho que não resolve
//! **não** executam nada; o turno para com erro e o log fica consistente.
//!
//! O **observador efémero** (`ActivitySink`, E10-T05) recebe deltas e tools em curso para o painel
//! da UI: nada disso entra no log nem no contexto do modelo.

mod catalog;
mod command;
mod plan;
mod router;
mod shell;
mod turn;

#[cfg(test)]
mod tests;

pub(crate) use command::{
    RunArgs, SYSTEM, build_provider, default_base, default_model, open_runtime, run,
};
pub(crate) use shell::dispatch as shell_dispatch;
pub(crate) use turn::{Activity, ActivitySink, Approval, ApprovalPrompt, run_turn, run_turn_with};

use katu_core::error::{Error, ToolOutcome};
use katu_core::kernel::{CallContext, CallId, MemoryWriteRequest, SessionError, memory_recall_use};
use katu_core::memory::Memory;
use katu_core::ports::{Env, Fs, Process};
use katu_core::provider::{ModelSpec, Provider, ProviderError, TokenUsage};
use katu_policy::{ApprovalRequest, Decision, ToolUse};
use katu_tools::recall::RecallTool;
use katu_tools::write::WriteNoteTool;
use serde_json::Value;

use crate::runtime::Runtime;

/// Portas que o loop precisa, passadas pela borda (a memória vem do runtime).
#[derive(Clone, Copy)]
pub(crate) struct Ports<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Porta de processos (`bash`).
    pub process: &'a dyn Process,
    /// Porta de ambiente (`bash`).
    pub env: &'a dyn Env,
}

/// Parâmetros de um turno.
pub(crate) struct TurnOptions {
    /// Modelo + grau de pensamento.
    pub model: ModelSpec,
    /// Instrução de sistema (prime), quando há.
    pub system: Option<String>,
    /// Teto de tokens de saída.
    pub max_tokens: u32,
    /// Temperatura.
    pub temperature: f32,
    /// Máximo de passos (chamadas de tool) por turno.
    pub max_steps: u32,
}

/// Pedido de execução de um turno (o que o loop precisa além do runtime).
#[derive(Clone, Copy)]
pub(crate) struct TurnRequest<'a> {
    /// Provider do endpoint de modelo.
    pub provider: &'a dyn Provider,
    /// Portas de I/O para as tools.
    pub ports: Ports<'a>,
    /// Objetivo do utilizador.
    pub goal: &'a str,
    /// Parâmetros do turno.
    pub options: &'a TurnOptions,
}

/// Resultado observável de um turno.
pub(crate) struct TurnReport {
    /// Passos dados (uma chamada ao modelo cada).
    pub steps: u32,
    /// Texto final acumulado do assistente.
    pub text: String,
    /// Tool calls executadas.
    pub calls: usize,
    /// Contabilização do provider (base `provider_reported`).
    pub usage: Option<TokenUsage>,
    /// `true` se o turno foi **cancelado** pelo utilizador (E: cancelamento cooperativo).
    pub cancelled: bool,
}

/// Falha do loop de turnos.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentError {
    /// Falha de transporte/protocolo do provider.
    #[error("provider: {0}")]
    Provider(#[from] ProviderError),
    /// Falha de transição/custo/log na sessão.
    #[error("sessão: {0}")]
    Session(#[from] SessionError),
    /// Falha de roteamento (tool desconhecida ou argumento inválido).
    #[error("roteador: {0}")]
    Route(#[from] router::RouteError),
    /// O turno excedeu o teto de passos sem terminar.
    #[error("turno excedeu {steps} passos sem terminar")]
    TooManySteps {
        /// Teto atingido.
        steps: u32,
    },
    /// Um worker paralelo de tool call terminou abruptamente (panic vindo de uma porta).
    #[error("tool call paralela terminou abruptamente")]
    Worker,
    /// O guard de loop cortou o turno (Q-12/F7): repetição patológica antes do teto de passos.
    #[error("loop detectado no passo {step} ({kind}): {reason}")]
    LoopDetected {
        /// Passo em que o detector disparou.
        step: u32,
        /// Detector que disparou (`cusum`/`sprt`).
        kind: &'static str,
        /// Motivo com a evidência (o que se repetiu).
        reason: String,
    },
}

impl From<AgentError> for Error {
    fn from(error: AgentError) -> Self {
        let _span = katu_core::trace_fn!("agent::from");

        match error {
            AgentError::Provider(source) => source.into(),
            AgentError::Route(router::RouteError::UnknownTool(name)) => {
                Self::invalid_input(format!("tool desconhecida: {name}"))
            }
            AgentError::Route(source) => Self::invalid_input(source.to_string()),
            AgentError::Session(source) => Self::internal(source.to_string()),
            AgentError::TooManySteps { steps } => {
                Self::internal(format!("turno excedeu {steps} passos sem terminar"))
            }
            AgentError::LoopDetected { reason, .. } => Self::conflict(reason),
            AgentError::Worker => {
                Self::internal("tool call paralela terminou abruptamente".to_string())
            }
        }
    }
}

/// Resultado de uma tool call: o efeito, o uso resolvido e, quando a política pede, o pedido de
/// aprovação humana (E07-T05, §33).
pub(crate) struct CallOutcome {
    /// Efeito a devolver ao modelo.
    pub outcome: ToolOutcome,
    /// Uso resolvido (presente quando há caminho pela política, para derivar a capacidade).
    pub use_: Option<ToolUse>,
    /// Pedido de aprovação, quando a política o exigiu.
    pub approval: Option<ApprovalRequest>,
}

/// Roteia e executa uma tool call, mantendo a ordem §42 (logar → política → efeito).
///
/// Devolve o resultado da tool (incluindo recusas) para o observador o poder mostrar (E10-T04).
fn execute_call(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    call: CallId,
    name: &str,
    args: &Value,
) -> Result<CallOutcome, AgentError> {
    let _span = katu_core::trace_fn!("agent::execute_call");

    if name == "memory" {
        return execute_memory(runtime, call, args);
    }
    let root = runtime.root().to_path_buf();
    let loaded = runtime.plan().cloned();
    let now = runtime.clock.now().as_millis();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: runtime.clock,
        root: &root,
    };
    let call_outcome = match router::route(&route_ports, &runtime.cwd, name, args, loaded.as_ref())?
    {
        router::Routed::Plan { use_, plan } => {
            let outcome = plan::execute(runtime, call, &use_, &plan)?;
            CallOutcome {
                outcome,
                use_: Some(use_),
                approval: None,
            }
        }
        router::Routed::Plain { use_, tool } => {
            let dispatch = runtime.session.tool_call(
                call,
                &use_,
                CallContext {
                    rules: &runtime.rules,
                    now_millis: now,
                    tool: tool.as_ref(),
                },
            )?;
            let approval = match &dispatch.decision {
                Decision::RequireApproval { request } => Some(request.clone()),
                _ => None,
            };
            CallOutcome {
                outcome: dispatch.outcome(),
                use_: Some(use_),
                approval,
            }
        }
        _ => {
            return Err(AgentError::Route(router::RouteError::UnknownTool(
                name.to_string(),
            )));
        }
    };
    Ok(call_outcome)
}

/// Executa a tool `memory` pelos caminhos de recall/escrita do gate de E05.
fn execute_memory(
    runtime: &mut Runtime<'_>,
    call: CallId,
    args: &Value,
) -> Result<CallOutcome, AgentError> {
    let _span = katu_core::trace_fn!("agent::execute_memory");

    let Runtime {
        session,
        memory,
        rules,
        cwd,
        clock,
        ..
    } = runtime;
    let now = clock.now().as_millis();
    let memory: &dyn Memory = &*memory;
    let call_outcome = match router::memory(args)? {
        router::Routed::MemoryRecall { req } => {
            let tool = RecallTool { memory, req };
            let dispatch = session.tool_call(
                call,
                &memory_recall_use(cwd),
                CallContext {
                    rules,
                    now_millis: now,
                    tool: &tool,
                },
            )?;
            CallOutcome {
                outcome: dispatch.outcome(),
                use_: None,
                approval: None,
            }
        }
        router::Routed::MemoryRecord { req } => {
            let tool = WriteNoteTool {
                memory,
                req: req.clone(),
            };
            let dispatch = session.memory_write(
                call,
                MemoryWriteRequest {
                    cwd,
                    req: &req,
                    memory,
                    rules,
                    now_millis: now,
                    tool: &tool,
                },
            )?;
            CallOutcome {
                outcome: dispatch.outcome(),
                use_: None,
                approval: None,
            }
        }
        router::Routed::Plain { .. } | router::Routed::Plan { .. } => {
            return Err(AgentError::Route(router::RouteError::UnknownTool(
                "memory".to_string(),
            )));
        }
    };
    Ok(call_outcome)
}
