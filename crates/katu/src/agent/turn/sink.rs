//! Sink do passo de streaming (E12-T05): acumula texto/calls e reencaminha a atividade efémera.
//!
//! Nada disto entra no log: o texto é logado pelo `run` e as tool calls são executadas pela ordem
//! §42. O sink só serve o painel de atividade e a acumulação do passo.

use katu_core::kernel::CallId;
use katu_core::provider::{Flow, ProviderEvent, ProviderSink};
use serde_json::Value;

use super::{Activity, ActivitySink};

/// Teto de caracteres dos argumentos crus mostrados ao utilizador (o painel é efémero).
const MAX_ARGS_CHARS: usize = 200;

/// Sink que acumula o turno e reencaminha a atividade efémera.
pub(super) struct TurnSink<'a> {
    pub(super) activity: &'a mut dyn ActivitySink,
    pub(super) text: String,
    pub(super) calls: Vec<(CallId, String, Value)>,
    pub(super) truncated: Vec<(CallId, String)>,
}

impl ProviderSink for TurnSink<'_> {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        let _span = katu_core::trace_fn!("agent::turn::sink::on_event");

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
            // L-Q2: uma tool call truncada não é executada; é fechada como indisponível e o loop
            // continua para o modelo reformular.
            ProviderEvent::ToolCallTruncated { call, name } => {
                self.activity.activity(Activity::Tool {
                    name: &name,
                    args: "<truncada>",
                });
                self.truncated.push((call, name));
            }
            _ => {}
        }
        Flow::Continue
    }
}

/// Renderiza os argumentos **crus** do modelo de forma compacta (transparência), com teto.
pub(super) fn render_args(arguments: &Value) -> String {
    let _span = katu_core::trace_fn!("agent::turn::sink::render_args");

    let raw = serde_json::to_string(arguments).unwrap_or_else(|_| "<inválido>".to_string());
    if raw.chars().count() <= MAX_ARGS_CHARS {
        return raw;
    }
    let head: String = raw.chars().take(MAX_ARGS_CHARS).collect();
    format!("{head}…")
}
