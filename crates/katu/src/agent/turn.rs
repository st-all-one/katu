//! Execução de um turno (E12-T05/E10): pedido ao provider, observação efémera e tools §42.

use katu_core::diag::{Level, events};
use katu_core::kernel::CallId;
use katu_core::provider::{Flow, ProviderEvent, ProviderRequest, ProviderSink, ToolDef};
use serde_json::Value;

use super::{AgentError, TurnOptions, TurnReport, TurnRequest, catalog, execute_call};
use crate::runtime::Runtime;

/// Evento **efémero** do turno (E10-T05): observação ao vivo, fora do log e do contexto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity<'a> {
    /// Delta de texto do assistente.
    Text(&'a str),
    /// Delta de raciocínio.
    Thinking(&'a str),
    /// Tool pedida pelo modelo.
    Tool {
        /// Nome ao modelo da tool.
        name: &'a str,
    },
    /// Tool concluída.
    ToolDone {
        /// Nome ao modelo da tool.
        name: &'a str,
    },
}

/// Consumidor de atividade efémera (implementado pela borda/TUI).
pub(crate) trait ActivitySink {
    /// Recebe um evento efémero.
    fn activity(&mut self, activity: Activity<'_>);
}

/// Observador que ignora tudo (`katu run`).
struct NoActivity;

impl ActivitySink for NoActivity {
    fn activity(&mut self, _activity: Activity<'_>) {}
}

/// Sink que acumula o turno e reencaminha a atividade efémera.
struct TurnSink<'a> {
    activity: &'a mut dyn ActivitySink,
    text: String,
    calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for TurnSink<'_> {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
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
                self.activity.activity(Activity::Tool { name: &name });
                self.calls.push((call, name, arguments));
            }
            _ => {}
        }
        Flow::Continue
    }
}

/// Monta o pedido ao provider para um passo (histórico = projeção do log).
fn build_request(
    runtime: &Runtime<'_>,
    options: &TurnOptions,
    tools: &[ToolDef],
) -> Result<ProviderRequest, AgentError> {
    Ok(ProviderRequest {
        model: options.model.clone(),
        system: options.system.clone(),
        messages: runtime.messages()?,
        tools: tools.to_vec(),
        max_tokens: Some(options.max_tokens),
        temperature: Some(options.temperature),
    })
}

/// Executa um turno completo sem observador externo.
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
) -> Result<TurnReport, AgentError> {
    run_turn_with(runtime, request, &mut NoActivity)
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
    let TurnRequest {
        provider,
        ports,
        goal,
        options,
    } = request;
    let _span = katu_core::span!(Level::Info, events::KERNEL_TURN);
    if !runtime.session.state().turn_open {
        runtime.begin_turn()?;
    }
    runtime.record_user(goal)?;
    let tools = catalog::tool_defs();
    let mut text = String::new();
    let mut calls = 0_usize;
    let mut usage = None;
    let mut steps = 0_u32;
    loop {
        steps = steps.saturating_add(1);
        let request = build_request(runtime, options, &tools)?;
        let mut sink = TurnSink {
            activity: &mut *activity,
            text: String::new(),
            calls: Vec::new(),
        };
        let outcome = provider.stream(&request, &mut sink)?;
        usage = outcome.usage.or(usage);
        let TurnSink {
            text: sink_text,
            calls: sink_calls,
            activity: _,
        } = sink;
        if !sink_text.is_empty() {
            runtime.record_assistant(&sink_text)?;
            text.push_str(&sink_text);
        }
        calls = calls.saturating_add(sink_calls.len());
        if sink_calls.is_empty() {
            let turn = runtime.turn();
            runtime.record_turn_end(turn)?;
            return Ok(TurnReport {
                steps,
                text,
                calls,
                usage,
            });
        }
        for (call, name, arguments) in sink_calls {
            execute_call(runtime, &ports, call, &name, &arguments)?;
            activity.activity(Activity::ToolDone { name: &name });
        }
        if steps >= options.max_steps {
            return Err(AgentError::TooManySteps { steps });
        }
    }
}
