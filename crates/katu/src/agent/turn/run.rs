//! Loop de passos do turno (E12-T05/E10): provider → sink efémero → tools §42.
//!
//! Vive num módulo filho para manter `turn.rs` sob o teto de linhas. O sink acumula o texto e as
//! tool calls de cada passo e reencaminha a atividade **efémera** (deltas, tools, argumentos crus);
//! nada disto entra no log. O cancelamento cooperativo fecha o turno de forma limpa.

use katu_core::diag::{Level, events};
use katu_core::kernel::CallId;
use katu_core::provider::{
    Flow, Provider, ProviderError, ProviderEvent, ProviderRequest, ProviderSink, TokenUsage,
};
use serde_json::Value;

use super::request::build_request;
use super::{Activity, ActivitySink, run_calls};
use crate::agent::{AgentError, Ports, TurnOptions, TurnReport, TurnRequest, catalog};
use crate::runtime::Runtime;

/// Sink que acumula o turno e reencaminha a atividade efémera.
struct TurnSink<'a> {
    activity: &'a mut dyn ActivitySink,
    text: String,
    calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for TurnSink<'_> {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        let _span = katu_core::trace_fn!("agent::turn::run::on_event");

        if self.activity.cancelled() {
            return Flow::Break;
        }
        match event {
            ProviderEvent::Text(delta) => {
                self.activity.activity(Activity::Text(&delta));
                self.text.push_str(&delta);
            }
            ProviderEvent::Thinking(delta) => {
                self.activity.activity(Activity::Thinking(&delta));
            }
            ProviderEvent::ToolCall {
                call,
                name,
                arguments,
            } => {
                let args = render_args(&arguments);
                self.activity.activity(Activity::Tool {
                    name: &name,
                    args: &args,
                });
                self.calls.push((call, name, arguments));
            }
            _ => {}
        }
        Flow::Continue
    }
}

/// Teto de caracteres dos argumentos crus mostrados ao utilizador (o painel é efémero).
const MAX_ARGS_CHARS: usize = 200;

/// Renderiza os argumentos **crus** do modelo de forma compacta (transparência), com teto.
fn render_args(arguments: &Value) -> String {
    let _span = katu_core::trace_fn!("agent::turn::run::render_args");

    let raw = serde_json::to_string(arguments).unwrap_or_else(|_| "<inválido>".to_string());
    if raw.chars().count() <= MAX_ARGS_CHARS {
        return raw;
    }
    let head: String = raw.chars().take(MAX_ARGS_CHARS).collect();
    format!("{head}…")
}

/// Executa um turno completo, reencaminhando atividade efémera ao `activity` (E10-T05).
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn_with(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
    activity: &mut dyn ActivitySink,
) -> Result<TurnReport, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::run_turn_with");

    let TurnRequest {
        provider,
        ports,
        goal,
        options,
    } = request;
    let _span = katu_core::fn_span!(Level::Info, events::KERNEL_TURN, "run::run_turn_with");
    if !runtime.session.state().turn_open {
        runtime.begin_turn()?;
    }
    runtime.record_user(goal)?;
    let accum = drive(runtime, &ports, provider, options, activity)?;
    finish(runtime, accum)
}

/// Corre o loop de passos até ao fim (natural ou cancelado) e devolve o acumulado.
fn drive(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    provider: &dyn Provider,
    options: &TurnOptions,
    activity: &mut dyn ActivitySink,
) -> Result<Accum, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::drive");

    let tools = catalog::tool_defs();
    let mut accum = Accum {
        text: String::new(),
        calls: 0,
        usage: None,
        steps: 0,
        cancelled: false,
    };
    loop {
        if activity.cancelled() {
            accum.cancelled = true;
            return Ok(accum);
        }
        accum.steps = accum.steps.saturating_add(1);
        let request = build_request(runtime, options, &tools)?;
        let step = stream_step(runtime, provider, &request, activity)?;
        accum.usage = step.usage.or(accum.usage);
        accum.text.push_str(&step.text);
        accum.calls = accum.calls.saturating_add(step.calls.len());
        if step.cancelled {
            accum.cancelled = true;
            return Ok(accum);
        }
        if step.calls.is_empty() {
            return Ok(accum);
        }
        if !run_calls(runtime, ports, step.calls, activity)? {
            accum.cancelled = true;
            return Ok(accum);
        }
        if let Some(prompt) = activity.steer() {
            runtime.record_user(&prompt)?;
            katu_core::event!(
                Level::Info,
                events::TUI_STEER,
                "chars" => prompt.chars().count()
            );
        }
        if accum.steps >= options.max_steps {
            return Err(AgentError::TooManySteps { steps: accum.steps });
        }
    }
}

/// Resultado de um passo de streaming (texto e tool calls acumulados).
struct Step {
    text: String,
    calls: Vec<(CallId, String, Value)>,
    usage: Option<TokenUsage>,
    cancelled: bool,
}

/// Corre um passo (uma chamada ao provider), tratando o cancelamento como fim **limpo**.
///
/// Um [`ProviderError::Cancelled`] (o sink devolveu [`Flow::Break`]) regista o texto parcial e
/// devolve `cancelled: true`; nunca é um erro do turno.
fn stream_step(
    runtime: &mut Runtime<'_>,
    provider: &dyn Provider,
    request: &ProviderRequest,
    activity: &mut dyn ActivitySink,
) -> Result<Step, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::stream_step");

    let mut sink = TurnSink {
        activity: &mut *activity,
        text: String::new(),
        calls: Vec::new(),
    };
    let outcome = match provider.stream(request, &mut sink) {
        Ok(outcome) => outcome,
        Err(ProviderError::Cancelled) => {
            let TurnSink { text, calls, .. } = sink;
            if !text.is_empty() {
                runtime.record_assistant(&text)?;
            }
            return Ok(Step {
                text,
                calls,
                usage: None,
                cancelled: true,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let TurnSink { text, calls, .. } = sink;
    if !text.is_empty() {
        runtime.record_assistant(&text)?;
    }
    Ok(Step {
        text,
        calls,
        usage: outcome.usage,
        cancelled: false,
    })
}

/// Acumulado do turno até ao fecho.
struct Accum {
    text: String,
    calls: usize,
    usage: Option<TokenUsage>,
    steps: u32,
    cancelled: bool,
}

/// Fecha o turno e devolve o relatório (comum ao fim natural e ao cancelamento).
fn finish(runtime: &mut Runtime<'_>, accum: Accum) -> Result<TurnReport, AgentError> {
    let _span = katu_core::fn_span!(Level::Debug, events::KERNEL_STOP, "run::finish");
    let turn = runtime.turn();
    runtime.record_turn_end(turn)?;
    Ok(TurnReport {
        steps: accum.steps,
        text: accum.text,
        calls: accum.calls,
        usage: accum.usage,
        cancelled: accum.cancelled,
    })
}
