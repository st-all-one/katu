//! Decodificação incremental dos eventos da Responses API.

use std::collections::BTreeMap;

use katu_core::diag::{Level, events};
use katu_core::evidence::EvidenceBasis;
use katu_core::kernel::CallId;
use katu_core::provider::{
    Flow, ProviderError, ProviderEvent, ProviderOutcome, ProviderSink, StopReason, TokenUsage,
};

use super::chunk::{Event, Item, Response, UsageJson};
use crate::wire::{Wiring, parse_arguments};

/// Ciclo de vida da decodificação.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Ainda sem dados.
    #[default]
    Idle,
    /// A receber deltas.
    Streaming,
    /// Terminou (`response.completed`/`[DONE]`).
    Done,
    /// Cancelado pelo `sink`.
    Cancelled,
}

/// Acumulador de uma tool call (por `item_id`).
#[derive(Debug, Default)]
struct ToolAccum {
    call_id: String,
    name: String,
    arguments: String,
}

/// Estado da decodificação da Responses API.
#[allow(
    clippy::struct_excessive_bools,
    reason = "acumuladores independentes: houve tool calls e já se anunciou o TTFT"
)]
#[derive(Debug, Default)]
pub(crate) struct ResponsesDecoder {
    usage: Option<TokenUsage>,
    tools: BTreeMap<String, ToolAccum>,
    tool_calls: bool,
    status: Status,
    announced: bool,
}

impl ResponsesDecoder {
    /// Novo decodificador.
    pub(crate) fn new() -> Self {
        let _span = katu_core::trace_fn!("responses::decode::new");

        Self::default()
    }

    /// Trata um evento decodificado.
    fn handle(
        &mut self,
        event: &Event,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "responses::decode::handle"
        );
        match event.kind.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = event.delta.as_deref().filter(|delta| !delta.is_empty()) {
                    return Ok(self.emit_text(sink, delta));
                }
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                if let Some(delta) = event.delta.as_deref().filter(|delta| !delta.is_empty()) {
                    return Ok(self.emit_thinking(sink, delta));
                }
            }
            "response.output_item.added" => {
                if let Some(item) = &event.item {
                    self.absorb_item(item);
                }
            }
            "response.function_call_arguments.delta" => {
                if let (Some(id), Some(delta)) = (event.item_id.as_deref(), event.delta.as_deref())
                    && let Some(tool) = self.tools.get_mut(id)
                {
                    tool.arguments.push_str(delta);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = &event.item
                    && item.kind.as_deref() == Some("function_call")
                    && let Some(id) = item.id.as_deref()
                {
                    if let (Some(tool), Some(arguments)) =
                        (self.tools.get_mut(id), item.arguments.as_ref())
                    {
                        tool.arguments.clone_from(arguments);
                    }
                    return self.emit_tool(id, sink);
                }
            }
            "response.completed" => {
                self.absorb_response(event.response.as_ref(), event.usage.as_ref());
                self.status = Status::Done;
                return self.finish_stream(sink);
            }
            "response.failed" | "error" => {
                let detail = event.delta.as_deref().unwrap_or("sem detalhe");
                return Err(ProviderError::Decode(format!("resposta falhou: {detail}")));
            }
            _ => {}
        }
        Ok(Flow::Continue)
    }

    /// Emite texto, propagando o cancelamento.
    fn emit_text(&mut self, sink: &mut dyn ProviderSink, delta: &str) -> Flow {
        let _span = katu_core::trace_fn!("responses::decode::emit_text");

        self.emit(sink, ProviderEvent::Text(delta.to_string()))
    }

    /// Emite raciocínio, propagando o cancelamento.
    fn emit_thinking(&mut self, sink: &mut dyn ProviderSink, delta: &str) -> Flow {
        let _span = katu_core::trace_fn!("responses::decode::emit_thinking");

        self.emit(sink, ProviderEvent::Thinking(delta.to_string()))
    }

    /// Emite um evento marcando o **TTFT** na primeira ocorrência.
    fn emit(&mut self, sink: &mut dyn ProviderSink, event: ProviderEvent) -> Flow {
        let _span = katu_core::trace_fn!("responses::decode::emit");

        if !self.announced {
            self.announced = true;
            katu_core::event!(Level::Debug, events::PROVIDER_TTFT);
        }
        let flow = sink.on_event(event);
        if matches!(flow, Flow::Break) {
            self.status = Status::Cancelled;
        }
        flow
    }

    /// Acumula um item `function_call`.
    fn absorb_item(&mut self, item: &Item) {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "responses::decode::absorb_item"
        );
        if item.kind.as_deref() != Some("function_call") {
            return;
        }
        let Some(id) = item.id.as_deref() else {
            return;
        };
        let entry = self.tools.entry(id.to_string()).or_default();
        if let Some(call_id) = item.call_id.as_deref().filter(|call| !call.is_empty()) {
            entry.call_id = call_id.to_string();
        }
        if let Some(name) = item.name.as_deref().filter(|name| !name.is_empty()) {
            entry.name = name.to_string();
        }
        if let Some(arguments) = &item.arguments {
            entry.arguments.push_str(arguments);
        }
    }

    /// Emite uma tool call acumulada (idempotente: remove do mapa).
    fn emit_tool(&mut self, id: &str, sink: &mut dyn ProviderSink) -> Result<Flow, ProviderError> {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "responses::decode::emit_tool"
        );
        let Some(tool) = self.tools.remove(id) else {
            return Ok(Flow::Continue);
        };
        let arguments = parse_arguments(&tool.arguments)?;
        let call = CallId::new(if tool.call_id.is_empty() {
            format!("call_{id}")
        } else {
            tool.call_id
        });
        self.tool_calls = true;
        Ok(self.emit(
            sink,
            ProviderEvent::ToolCall {
                call,
                name: tool.name,
                arguments,
            },
        ))
    }

    /// Absorve o `usage` de `response.completed` (aceita topo ou dentro de `response`).
    fn absorb_response(&mut self, response: Option<&Response>, top: Option<&UsageJson>) {
        let _span = katu_core::trace_fn!("responses::decode::absorb_response");

        if let Some(usage) = response
            .and_then(|response| response.usage.as_ref())
            .or(top)
        {
            self.absorb_usage(usage);
        }
    }

    /// Absorve a contabilização (base `provider_reported`).
    fn absorb_usage(&mut self, json: &UsageJson) {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "responses::decode::absorb_usage"
        );
        let mut usage = self
            .usage
            .unwrap_or_else(|| TokenUsage::new(EvidenceBasis::ProviderReported));
        usage.basis = EvidenceBasis::ProviderReported;
        usage.input = json.input_tokens.or(usage.input);
        usage.output = json.output_tokens.or(usage.output);
        usage.cached_input = json
            .input_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens)
            .or(usage.cached_input);
        usage.reasoning = json
            .output_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens)
            .or(usage.reasoning);
        self.usage = Some(usage);
    }
}

impl Wiring for ResponsesDecoder {
    fn feed_payload(
        &mut self,
        payload: &str,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        let _span = katu_core::trace_fn!("responses::decode::feed_payload");

        if matches!(self.status, Status::Idle) {
            self.status = Status::Streaming;
        }
        let payload = payload.trim();
        if payload == "[DONE]" {
            self.status = Status::Done;
            return self.finish_stream(sink);
        }
        let event: Event = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Decode(error.to_string()))?;
        self.handle(&event, sink)
    }

    fn finish_stream(&mut self, sink: &mut dyn ProviderSink) -> Result<Flow, ProviderError> {
        let _span = katu_core::trace_fn!("responses::decode::finish_stream");

        let ids: Vec<String> = self.tools.keys().cloned().collect();
        for id in ids {
            if matches!(self.emit_tool(&id, sink)?, Flow::Break) {
                return Ok(Flow::Break);
            }
        }
        Ok(Flow::Continue)
    }

    fn has_emitted(&self) -> bool {
        let _span = katu_core::trace_fn!("responses::decode::has_emitted");

        self.announced
    }

    fn has_data(&self) -> bool {
        let _span = katu_core::trace_fn!("responses::decode::has_data");

        !matches!(self.status, Status::Idle)
    }

    fn is_cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("responses::decode::is_cancelled");

        matches!(self.status, Status::Cancelled)
    }

    fn is_done(&self) -> bool {
        let _span = katu_core::trace_fn!("responses::decode::is_done");

        matches!(self.status, Status::Done)
    }

    fn final_outcome(&self) -> ProviderOutcome {
        let _span = katu_core::trace_fn!("responses::decode::final_outcome");

        ProviderOutcome {
            usage: self.usage,
            stop: if self.tool_calls {
                StopReason::ToolCalls
            } else {
                StopReason::EndTurn
            },
        }
    }
}
