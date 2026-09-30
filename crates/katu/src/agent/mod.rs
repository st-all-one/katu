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
mod turn;

#[cfg(test)]
mod tests;

pub(crate) use command::{RunArgs, SYSTEM, build_provider, default_base, default_model, run};
pub(crate) use turn::{Activity, ActivitySink, run_turn, run_turn_with};

use katu_core::error::Error;
use katu_core::kernel::{CallContext, CallId, MemoryWriteRequest, SessionError, memory_recall_use};
use katu_core::memory::Memory;
use katu_core::ports::{Env, Fs, Process};
use katu_core::provider::{ModelSpec, Provider, ProviderError, TokenUsage};
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
}

impl From<AgentError> for Error {
    fn from(error: AgentError) -> Self {
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
        }
    }
}

/// Roteia e executa uma tool call, mantendo a ordem §42 (logar → política → efeito).
fn execute_call(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    call: CallId,
    name: &str,
    args: &Value,
) -> Result<(), AgentError> {
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
    match router::route(&route_ports, &runtime.cwd, name, args, loaded.as_ref())? {
        router::Routed::Plan { use_, plan } => plan::execute(runtime, call, &use_, &plan)?,
        router::Routed::Plain { use_, tool } => {
            runtime.session.tool_call(
                call,
                &use_,
                CallContext {
                    rules: &runtime.rules,
                    now_millis: now,
                    tool: tool.as_ref(),
                },
            )?;
        }
        _ => {
            return Err(AgentError::Route(router::RouteError::UnknownTool(
                name.to_string(),
            )));
        }
    }
    Ok(())
}

/// Executa a tool `memory` pelos caminhos de recall/escrita do gate de E05.
fn execute_memory(runtime: &mut Runtime<'_>, call: CallId, args: &Value) -> Result<(), AgentError> {
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
    match router::memory(args)? {
        router::Routed::MemoryRecall { req } => {
            let tool = RecallTool { memory, req };
            session.tool_call(
                call,
                &memory_recall_use(cwd),
                CallContext {
                    rules,
                    now_millis: now,
                    tool: &tool,
                },
            )?;
        }
        router::Routed::MemoryRecord { req } => {
            let tool = WriteNoteTool {
                memory,
                req: req.clone(),
            };
            session.memory_write(
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
        }
        router::Routed::Plain { .. } | router::Routed::Plan { .. } => {
            return Err(AgentError::Route(router::RouteError::UnknownTool(
                "memory".to_string(),
            )));
        }
    }
    Ok(())
}
