//! Sink trivial que acumula o stream (testes/sondagens), separado por limite de ficheiro.

use serde_json::Value;

use super::{Flow, ProviderEvent, ProviderSink};
use crate::kernel::CallId;

/// Implementação trivial que só mantém o último delta (útil em testes e sondagens).
#[derive(Debug, Default)]
pub struct CollectSink {
    /// Texto acumulado.
    pub text: String,
    /// Raciocínio acumulado.
    pub thinking: String,
    /// Tool calls completas, na ordem de chegada.
    pub calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for CollectSink {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        let _span = crate::trace_fn!("provider::collect::on_event");

        match event {
            ProviderEvent::Text(delta) => self.text.push_str(&delta),
            ProviderEvent::Thinking(delta) => self.thinking.push_str(&delta),
            ProviderEvent::ToolCall {
                call,
                name,
                arguments,
            } => self.calls.push((call, name, arguments)),
        }
        Flow::Continue
    }
}
