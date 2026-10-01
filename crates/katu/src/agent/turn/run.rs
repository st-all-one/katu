//! Loop de passos do turno (E12-T05/E10): provider → sink efémero → tools §42.
//!
//! Vive num módulo filho para manter `turn.rs` sob o teto de linhas. O sink acumula o texto e as
//! tool calls de cada passo e reencaminha a atividade **efémera** (deltas, tools, argumentos crus);
//! nada disto entra no log. O cancelamento cooperativo fecha o turno de forma limpa.

use katu_core::diag::{Level, events};
use katu_core::kernel::{Call, CallId, Fingerprint, Guard};
use katu_core::provider::{
    Flow, Provider, ProviderError, ProviderEvent, ProviderRequest, ProviderSink, TokenUsage,
};
use serde_json::Value;

use super::request::build_request;
use super::{Activity, ActivitySink, run_calls};
use crate::agent::{AgentError, Ports, TurnOptions, TurnReport, TurnRequest, catalog};
use crate::defaults;
use crate::runtime::Runtime;
use katu_tools::schema::concurrency_of;

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
    // Q-04: a secção `estado` do turno entra no log **antes** de qualquer pedido ao provider, pelo
    // que o prompt de sistema que o modelo vê é reconstruível do log (E04).
    runtime.record_prompt_state(options.max_steps)?;
    // Q-12: o turno fecha **sempre**, mesmo quando o loop guard corta a meio (ou o teto de passos
    // estoura): um `TurnStart` sem `TurnEnd` deixaria o log inconsistente para a retomada.
    let driven = drive(runtime, &ports, provider, options, activity);
    match driven {
        Ok(accum) => finish(runtime, accum),
        Err(error) => {
            let turn = runtime.turn();
            runtime.record_turn_end(turn)?;
            Err(error)
        }
    }
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
    let mut guard = Guard::with_defaults();
    // A3/W8-4: gate de Value of Information — **off** por omissão até A/B com o modelo.
    let mut voi = super::voi::Voi::new();
    let voi_enabled = defaults::from_root(runtime.root())
        .tool_voi
        .unwrap_or(false);
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
        // Q-12/F7: o guard observa as chamadas do passo **antes** de as executar (corta primeiro).
        cut_if_looping(&mut guard, &step.calls)?;
        // A3/W8-4: o gate de VOI não repete uma só-leitura já satisfeita (nunca o irreconstruível).
        let calls = if voi_enabled {
            super::voi::apply(runtime, ports, &mut voi, step.calls)?
        } else {
            step.calls
        };
        if !run_calls(runtime, ports, calls, activity)? {
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

/// Observa o passo no guard e **corta** o turno em ciclo (Q-12/F7).
///
/// O corte é registrado (`agent.loop`) e devolvido como erro: nunca silencioso. As chamadas são
/// observadas **antes** de correr, pelo que o ciclo não chega a gastar orçamento de tools.
fn cut_if_looping(guard: &mut Guard, calls: &[(CallId, String, Value)]) -> Result<(), AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::cut_if_looping");

    let Some(alarm) = guard.observe(&fingerprints(calls)) else {
        return Ok(());
    };
    katu_core::event!(
        Level::Warn,
        events::AGENT_LOOP,
        "step" => alarm.step,
        "kind" => alarm.kind.as_str(),
        "repeated" => alarm.repeated,
        "novelty_milli" => alarm.novelty_milli,
        "cusum_milli" => alarm.cusum_milli,
        "e_value_log_milli" => alarm.e_value_log_milli
    );
    Err(AgentError::LoopDetected {
        step: alarm.step,
        kind: alarm.kind.as_str(),
        reason: alarm.reason(),
    })
}

/// Traduz as chamadas do passo em impressões para o guard (Q-12).
///
/// Uma chamada **exclusiva** (escreve, move, executa) marca o passo como **progresso**: o detector
/// reinicia. É o que distingue um ciclo patológico de um *polling* legítimo.
fn fingerprints(calls: &[(CallId, String, Value)]) -> Vec<Call> {
    let _span = katu_core::trace_fn!("agent::turn::run::fingerprints");

    calls
        .iter()
        .map(|(_, name, arguments)| {
            let print = Fingerprint::of(name, arguments);
            if concurrency_of(name).is_shared() {
                Call::shared(print)
            } else {
                Call::exclusive(print)
            }
        })
        .collect()
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
