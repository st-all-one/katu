//! Decodificação incremental do dialeto `messages` (Anthropic).

use std::collections::BTreeMap;

use katu_core::diag::{Level, events};
use katu_core::evidence::EvidenceBasis;
use katu_core::kernel::CallId;
use katu_core::provider::{
    Flow, ProviderError, ProviderEvent, ProviderOutcome, ProviderSink, StopReason, TokenUsage,
};

use super::chunk::{Event, UsageJson};
use crate::wire::{Wiring, parse_arguments};

/// Ciclo de vida da decodificação.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Ainda sem dados.
    #[default]
    Idle,
    /// A receber deltas.
    Streaming,
    /// Terminou (`message_stop`).
    Done,
    /// Cancelado pelo `sink`.
    Cancelled,
}

/// Acumulador de uma tool call (por índice de bloco).
#[derive(Debug, Default)]
struct ToolAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Estado da decodificação Anthropic Messages.
#[allow(
    clippy::struct_excessive_bools,
    reason = "acumuladores independentes: houve tool calls e já se anunciou o TTFT"
)]
#[derive(Debug, Default)]
pub(crate) struct MessagesDecoder {
    usage: Option<TokenUsage>,
    tools: BTreeMap<u32, ToolAccum>,
    tool_calls: bool,
    stop: Option<StopReason>,
    status: Status,
    announced: bool,
}

impl MessagesDecoder {
    /// Novo decodificador.
    pub(crate) fn new() -> Self {
        let _span = katu_core::trace_fn!("anthropic::decode::new");

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
            "anthropic::decode::handle"
        );
        match event.kind.as_str() {
            "message_start" => {
                if let Some(usage) = event.message.as_ref().and_then(|m| m.usage.as_ref()) {
                    self.absorb_usage(usage);
                }
            }
            "content_block_start" => {
                if let Some(block) = &event.content_block
                    && block.kind.as_deref() == Some("tool_use")
                {
                    let entry = self.tools.entry(event.index.unwrap_or(0)).or_default();
                    if let Some(id) = block.id.as_deref().filter(|id| !id.is_empty()) {
                        entry.id = id.to_string();
                    }
                    if let Some(name) = block.name.as_deref().filter(|name| !name.is_empty()) {
                        entry.name = name.to_string();
                    }
                }
            }
            "content_block_delta" => {
                return Ok(self.handle_delta(event, sink));
            }
            "content_block_stop" => {
                return self.emit_tool(event.index.unwrap_or(0), sink);
            }
            "message_delta" => {
                if let Some(usage) = &event.usage {
                    self.absorb_usage(usage);
                }
                if let Some(reason) = event.delta.as_ref().and_then(|d| d.stop_reason.as_deref()) {
                    self.stop = Some(map_stop(reason));
                }
            }
            "message_stop" => {
                self.status = Status::Done;
                return self.finish_stream(sink);
            }
            "error" => {
                return Err(ProviderError::Decode(
                    "erro no stream Anthropic".to_string(),
                ));
            }
            _ => {}
        }
        Ok(Flow::Continue)
    }

    /// Trata `content_block_delta` (texto, raciocínio ou argumentos de tool).
    fn handle_delta(&mut self, event: &Event, sink: &mut dyn ProviderSink) -> Flow {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "anthropic::decode::handle_delta"
        );
        let Some(delta) = &event.delta else {
            return Flow::Continue;
        };
        match delta.kind.as_deref() {
            Some("text_delta") => {
                if let Some(text) = delta.text.as_deref().filter(|text| !text.is_empty()) {
                    return self.emit(sink, ProviderEvent::Text(text.to_string()));
                }
            }
            Some("thinking_delta") => {
                if let Some(text) = delta.thinking.as_deref().filter(|text| !text.is_empty()) {
                    return self.emit(sink, ProviderEvent::Thinking(text.to_string()));
                }
            }
            Some("input_json_delta") => {
                if let (Some(index), Some(partial)) = (event.index, delta.partial_json.as_deref())
                    && let Some(tool) = self.tools.get_mut(&index)
                {
                    tool.arguments.push_str(partial);
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    /// Emite um evento marcando o **TTFT** na primeira ocorrência.
    fn emit(&mut self, sink: &mut dyn ProviderSink, event: ProviderEvent) -> Flow {
        let _span = katu_core::trace_fn!("anthropic::decode::emit");

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

    /// Emite a tool call acumulada num índice (idempotente).
    fn emit_tool(
        &mut self,
        index: u32,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "anthropic::decode::emit_tool"
        );
        let Some(tool) = self.tools.remove(&index) else {
            return Ok(Flow::Continue);
        };
        let arguments = parse_arguments(&tool.arguments)?;
        let call = CallId::new(if tool.id.is_empty() {
            format!("call_{index}")
        } else {
            tool.id
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

    /// Absorve a contabilização (base `provider_reported`).
    fn absorb_usage(&mut self, json: &UsageJson) {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::PROVIDER_CHUNK,
            "anthropic::decode::absorb_usage"
        );
        let mut usage = self
            .usage
            .unwrap_or_else(|| TokenUsage::new(EvidenceBasis::ProviderReported));
        usage.basis = EvidenceBasis::ProviderReported;
        usage.input = json.input_tokens.or(usage.input);
        usage.output = json.output_tokens.or(usage.output);
        usage.cached_input = json.cache_read_input_tokens.or(usage.cached_input);
        self.usage = Some(usage);
    }
}

impl Wiring for MessagesDecoder {
    fn feed_payload(
        &mut self,
        payload: &str,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        let _span = katu_core::trace_fn!("anthropic::decode::feed_payload");

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
        let _span = katu_core::trace_fn!("anthropic::decode::finish_stream");

        let indices: Vec<u32> = self.tools.keys().copied().collect();
        for index in indices {
            if matches!(self.emit_tool(index, sink)?, Flow::Break) {
                return Ok(Flow::Break);
            }
        }
        Ok(Flow::Continue)
    }

    fn has_emitted(&self) -> bool {
        let _span = katu_core::trace_fn!("anthropic::decode::has_emitted");

        self.announced
    }

    fn has_data(&self) -> bool {
        let _span = katu_core::trace_fn!("anthropic::decode::has_data");

        !matches!(self.status, Status::Idle)
    }

    fn is_cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("anthropic::decode::is_cancelled");

        matches!(self.status, Status::Cancelled)
    }

    fn is_done(&self) -> bool {
        let _span = katu_core::trace_fn!("anthropic::decode::is_done");

        matches!(self.status, Status::Done)
    }

    fn final_outcome(&self) -> ProviderOutcome {
        let _span = katu_core::trace_fn!("anthropic::decode::final_outcome");

        ProviderOutcome {
            usage: self.usage,
            stop: self.stop.clone().unwrap_or(if self.tool_calls {
                StopReason::ToolCalls
            } else {
                StopReason::EndTurn
            }),
        }
    }
}

/// Mapeia o `stop_reason` do dialeto.
fn map_stop(reason: &str) -> StopReason {
    let _span = katu_core::trace_fn!("anthropic::decode::map_stop");

    match reason {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolCalls,
        "max_tokens" => StopReason::Length,
        "refusal" => StopReason::ContentFilter,
        other => StopReason::Other(other.to_string()),
    }
}
