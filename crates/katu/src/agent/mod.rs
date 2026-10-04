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
mod failure;
mod model;
mod plan;
mod router;
mod shell;
mod turn;

#[cfg(test)]
mod tests;

pub(crate) use command::{
    RunArgs, SYSTEM, build_provider, default_base, default_model, open_runtime, run,
};
pub(crate) use failure::{route_failure, settle_truncated};
pub(crate) use model::StepModel;
pub(crate) use shell::dispatch as shell_dispatch;
#[cfg(test)]
pub(crate) use turn::run_turn;
pub(crate) use turn::{Activity, ActivitySink, Approval, ApprovalPrompt, run_turn_with};

use katu_core::error::{Error, ToolOutcome};
use katu_core::kernel::{CallContext, CallId, SessionError, memory_recall_use};
use katu_core::memory::Memory;
use katu_core::ports::{Cancel, Env, Fs, Process, Progress};
use katu_core::provider::{ModelSpec, Provider, ProviderError, StopReason, TokenUsage};
use katu_policy::{ApprovalRequest, Decision, ToolUse};
use katu_tools::recall::RecallTool;
use serde_json::Value;

use crate::runtime::{Runtime, RuntimeError};

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

/// Teto de inatividade por omissão do stream do provider (L-P2), conservador: um stream que não
/// produz nada durante 60 s é um *stall* recuperável.
pub(crate) const DEFAULT_IDLE_MS: u64 = 60_000;

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
    /// Teto de inatividade do stream do provider em ms (L-P2); `0` desliga o *stall*.
    pub idle_ms: u64,
    /// Resolve o modelo de **cada passo** (`Q2/PI_GAINS`); `None` mantém [`TurnOptions::model`] fixo.
    pub step_model: Option<Box<dyn StepModel>>,
}

/// Pedido de execução de um turno (o que o loop precisa além do runtime).
#[derive(Clone)]
pub(crate) struct TurnRequest<'a> {
    /// Provider do endpoint de modelo (partilhado: o I/O corre fora da thread da UI, L-P1).
    pub provider: std::sync::Arc<dyn Provider>,
    /// Portas de I/O para as tools.
    pub ports: Ports<'a>,
    /// Objetivo do utilizador.
    pub goal: &'a str,
    /// Parâmetros do turno.
    pub options: &'a TurnOptions,
    /// Cancelamento cooperativo do turno (L-P3); `None` = nunca cancela.
    pub cancel: Option<&'a dyn Cancel>,
    /// Progresso efémero do output das tools (`P1/PI_GAINS`); nunca entra no log.
    pub progress: &'a dyn Progress,
}

/// Resultado observável de um turno.
pub(crate) struct TurnReport {
    /// Modelo usado no **último** passo (`Q2/PI_GAINS`; pode mudar a meio do turno).
    pub model: String,
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
    /// Motivo de paragem reportado pelo provider no último passo (L-Q2).
    pub stop: StopReason,
    /// Como o turno terminou (L-Q3): o envelope de máquina distingue o fim anormal.
    pub termination: Termination,
}

/// Como o turno terminou (L-Q3).
///
/// O fim **anormal** fecha o turno com uma mensagem visível ao utilizador e mantém o motivo no
/// envelope de máquina — nunca em silêncio, nunca como falha fatal para o humano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Termination {
    /// Fim natural (texto final ou tool calls concluídas).
    Natural,
    /// Fim pedido por uma tool **terminal** (`Q1/PI_GAINS`): o verbo cortou o loop.
    Terminal,
    /// Cancelado pelo utilizador.
    Cancelled,
    /// Resposta vazia mesmo depois dos retries (orçamento de saída gasto em raciocínio).
    Empty,
    /// Teto de passos atingido sem terminar.
    MaxSteps {
        /// Teto atingido.
        steps: u32,
    },
    /// Loop conversacional cortado pelo guard (Q-12/F7).
    Loop {
        /// Passo em que o detector disparou.
        step: u32,
        /// Motivo com a evidência (o que se repetiu).
        reason: String,
    },
}

impl Termination {
    /// Nome estável para o envelope de máquina.
    #[must_use]
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::Terminal => "terminal",
            Self::Cancelled => "cancelled",
            Self::Empty => "empty",
            Self::MaxSteps { .. } => "max_steps",
            Self::Loop { .. } => "loop",
        }
    }

    /// Código de rodada do envelope (0 = fim natural/cancelado; >0 = fim anormal).
    #[must_use]
    pub(crate) const fn round_exit(&self) -> u8 {
        match self {
            Self::Natural | Self::Terminal | Self::Cancelled => 0,
            Self::Empty => 1,
            Self::MaxSteps { .. } => 70,
            Self::Loop { .. } => 5,
        }
    }

    /// Mensagem visível ao utilizador (ausente no fim natural).
    #[must_use]
    pub(crate) fn message(&self) -> Option<String> {
        let _span = katu_core::trace_fn!("agent::termination::message");

        match self {
            Self::Natural | Self::Terminal | Self::Cancelled => None,
            Self::Empty => Some(
                "não consegui produzir uma resposta em texto (o orçamento de saída pode ter sido \
                 gasto em raciocínio); aumenta `--max-tokens` ou muda de modelo"
                    .to_string(),
            ),
            Self::MaxSteps { steps } => Some(format!(
                "atingi o limite de {steps} passos sem terminar; responda para eu continuar"
            )),
            Self::Loop { reason, .. } => Some(format!(
                "cortei o turno por repetição ({reason}); responda para eu continuar de outra forma"
            )),
        }
    }
}

/// Rótulo estável do motivo de paragem do provider (L-Q2).
#[must_use]
pub(crate) fn stop_label(stop: &StopReason) -> &'static str {
    let _span = katu_core::trace_fn!("agent::stop_label");

    match stop {
        StopReason::EndTurn => "end_turn",
        StopReason::ToolCalls => "tool_calls",
        StopReason::Length => "length",
        StopReason::ContentFilter => "content_filter",
        StopReason::Other(_) | _ => "other",
    }
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
    /// Falha do runtime (memória, recall, escrita).
    #[error("runtime: {0}")]
    Runtime(#[from] RuntimeError),
    /// Falha de roteamento (tool desconhecida ou argumento inválido).
    #[error("roteador: {0}")]
    Route(#[from] router::RouteError),
    /// Um worker paralelo de tool call terminou abruptamente (panic vindo de uma porta).
    #[error("tool call paralela terminou abruptamente")]
    Worker,
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
            AgentError::Runtime(source) => Self::internal(source.to_string()),
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
    /// Delta **model-visible** do efeito (resumo de uma linha para o painel; `LIVE_FLOW` LF4).
    pub delta: Option<String>,
    /// `true` se a tool pediu o fim normal do turno (`Q1/PI_GAINS`).
    pub terminate: bool,
}

/// Roteia e executa uma tool call, mantendo a ordem §42 (logar → política → efeito).
///
/// Devolve o resultado da tool (incluindo recusas) para o observador o poder mostrar (E10-T04).
#[allow(
    clippy::too_many_arguments,
    reason = "runtime + portas + cancelamento + identidade da call + nome + argumentos são o contexto mínimo do roteamento §42"
)]
fn execute_call(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    cancel: Option<&dyn Cancel>,
    progress: &dyn Progress,
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
        cancel,
        progress,
    };
    let routed = match router::route(&route_ports, &runtime.cwd, name, args, loaded.as_ref()) {
        Ok(routed) => routed,
        // Argumentos do modelo malformados: devolve o erro **ao modelo** (não aborta o turno).
        Err(error) => return route_failure(runtime, call, name, &error, now),
    };
    let call_outcome = match routed {
        router::Routed::Plan { use_, plan } => {
            let outcome = plan::execute(runtime, call, &use_, &plan)?;
            CallOutcome {
                outcome,
                use_: Some(use_),
                approval: None,
                delta: None,
                terminate: false,
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
                delta: dispatch.delta(),
                terminate: dispatch.terminate(),
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
                delta: dispatch.delta(),
                terminate: dispatch.terminate(),
            }
        }
        router::Routed::MemoryRecord { req } => {
            // Usa o caminho de escrita do runtime (recall prévio + gate de E05)
            let dispatch = runtime.remember(&req)?;
            CallOutcome {
                outcome: dispatch.outcome(),
                use_: None,
                approval: None,
                delta: dispatch.delta(),
                terminate: dispatch.terminate(),
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
