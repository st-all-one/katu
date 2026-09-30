//! Loop de turnos do agente (E12-T05/E10): provider ↔ kernel, tool execution pela ordem §42.
//!
//! O provider só **fala** com o endpoint de modelo; o kernel é o dono do loop. Cada pedido do
//! modelo é logado **antes** de executar (`ToolCall`), a política decide e só então o executor
//! corre (`ToolResult`) — `Session::tool_call` garante a ordem. O histórico enviado ao modelo é a
//! projeção do log (`derive_messages`), pelo que `Model-visible ⟺ logged` se mantém (E04).
//!
//! Fail-closed: uma tool fora do catálogo, um argumento em falta ou um caminho que não resolve
//! **não** executam nada; o turno para com erro e o log fica consistente.

mod catalog;
mod command;
mod router;

#[cfg(test)]
mod tests;

pub(crate) use command::{RunArgs, run};

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::{CallContext, CallId, MemoryWriteRequest, SessionError, memory_recall_use};
use katu_core::memory::Memory;
use katu_core::ports::{Env, Fs, Process};
use katu_core::provider::{
    Flow, ModelSpec, Provider, ProviderError, ProviderEvent, ProviderRequest, ProviderSink,
    TokenUsage,
};
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

/// Sink que acumula o texto e as tool calls do turno.
#[derive(Default)]
struct TurnSink {
    text: String,
    calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for TurnSink {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        match event {
            ProviderEvent::Text(delta) => self.text.push_str(&delta),
            ProviderEvent::ToolCall {
                call,
                name,
                arguments,
            } => self.calls.push((call, name, arguments)),
            _ => {}
        }
        Flow::Continue
    }
}

/// Executa um turno completo: mensagem do utilizador → (modelo → tools)* → paragem.
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn(
    runtime: &mut Runtime<'_>,
    provider: &dyn Provider,
    ports: &Ports<'_>,
    goal: &str,
    options: &TurnOptions,
) -> Result<TurnReport, AgentError> {
    let _span = katu_core::span!(Level::Info, events::KERNEL_TURN);
    runtime.record_user(goal)?;
    let tools = catalog::tool_defs();
    let mut text = String::new();
    let mut calls = 0_usize;
    let mut usage = None;
    let mut steps = 0_u32;
    loop {
        steps = steps.saturating_add(1);
        let request = ProviderRequest {
            model: options.model.clone(),
            system: options.system.clone(),
            messages: runtime.messages()?,
            tools: tools.clone(),
            max_tokens: Some(options.max_tokens),
            temperature: Some(options.temperature),
        };
        let mut sink = TurnSink::default();
        let outcome = provider.stream(&request, &mut sink)?;
        usage = outcome.usage.or(usage);
        if !sink.text.is_empty() {
            runtime.record_assistant(&sink.text)?;
            text.push_str(&sink.text);
        }
        calls = calls.saturating_add(sink.calls.len());
        if sink.calls.is_empty() {
            let turn = runtime.turn();
            runtime.record_turn_end(turn)?;
            return Ok(TurnReport {
                steps,
                text,
                calls,
                usage,
            });
        }
        for (call, name, arguments) in sink.calls {
            execute_call(runtime, ports, call, &name, &arguments)?;
        }
        if steps >= options.max_steps {
            return Err(AgentError::TooManySteps { steps });
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
    let Runtime {
        session,
        rules,
        cwd,
        clock,
        ..
    } = runtime;
    let now = clock.now().as_millis();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: *clock,
        root: &root,
    };
    let router::Routed::Plain { use_, tool } = router::route(&route_ports, cwd, name, args)? else {
        return Err(AgentError::Route(router::RouteError::UnknownTool(
            name.to_string(),
        )));
    };
    session.tool_call(
        call,
        &use_,
        CallContext {
            rules,
            now_millis: now,
            tool: tool.as_ref(),
        },
    )?;
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
        router::Routed::Plain { .. } => {
            return Err(AgentError::Route(router::RouteError::UnknownTool(
                "memory".to_string(),
            )));
        }
    }
    Ok(())
}
