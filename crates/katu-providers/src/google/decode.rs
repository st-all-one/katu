//! Decodificação incremental do dialeto Google Gemini (`streamGenerateContent`).

use katu_core::diag::{Level, events};
use katu_core::evidence::EvidenceBasis;
use katu_core::kernel::CallId;
use katu_core::provider::{
    Flow, ProviderError, ProviderEvent, ProviderOutcome, ProviderSink, StopReason, TokenUsage,
};
use serde_json::{Map, Value};

use super::chunk::{Chunk, UsageJson};
use crate::wire::Wiring;

/// Ciclo de vida da decodificação.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Ainda sem dados.
    #[default]
    Idle,
    /// A receber deltas.
    Streaming,
    /// Terminou.
    Done,
    /// Cancelado pelo `sink`.
    Cancelled,
}

/// Estado da decodificação Google Gemini.
#[allow(
    clippy::struct_excessive_bools,
    reason = "acumuladores independentes: houve tool calls e já se anunciou o TTFT"
)]
#[derive(Debug, Default)]
pub(crate) struct GoogleDecoder {
    usage: Option<TokenUsage>,
    tool_calls: bool,
    stop: Option<StopReason>,
    status: Status,
    announced: bool,
    sequence: u32,
}

impl GoogleDecoder {
    /// Novo decodificador.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Trata um `Chunk` do stream.
    fn handle(&mut self, chunk: &Chunk, sink: &mut dyn ProviderSink) -> Flow {
        if let Some(usage) = &chunk.usage_metadata {
            self.absorb_usage(usage);
        }
        let mut finished = false;
        for candidate in &chunk.candidates {
            if let Some(reason) = candidate.finish_reason.as_deref() {
                self.stop = Some(map_stop(reason));
                finished = true;
            }
            let Some(content) = &candidate.content else {
                continue;
            };
            for part in &content.parts {
                if let Some(call) = &part.function_call {
                    if matches!(self.emit_call(call, sink), Flow::Break) {
                        return Flow::Break;
                    }
                } else if let Some(text) = part.text.as_deref().filter(|text| !text.is_empty()) {
                    let event = if part.thought {
                        ProviderEvent::Thinking(text.to_string())
                    } else {
                        ProviderEvent::Text(text.to_string())
                    };
                    if matches!(self.emit(sink, event), Flow::Break) {
                        return Flow::Break;
                    }
                }
            }
        }
        if finished {
            self.status = Status::Done;
        }
        Flow::Continue
    }

    /// Emite uma chamada de função (o `args` do Gemini chega completo).
    fn emit_call(
        &mut self,
        call: &super::chunk::FunctionCall,
        sink: &mut dyn ProviderSink,
    ) -> Flow {
        self.tool_calls = true;
        let id = call
            .id
            .clone()
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                self.sequence = self.sequence.saturating_add(1);
                format!("call_{}", self.sequence)
            });
        let arguments = if call.args.is_null() {
            Value::Object(Map::new())
        } else {
            call.args.clone()
        };
        self.emit(
            sink,
            ProviderEvent::ToolCall {
                call: CallId::new(id),
                name: call.name.clone(),
                arguments,
            },
        )
    }

    /// Emite um evento marcando o **TTFT** na primeira ocorrência.
    fn emit(&mut self, sink: &mut dyn ProviderSink, event: ProviderEvent) -> Flow {
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

    /// Absorve a contabilização (base `provider_reported`).
    fn absorb_usage(&mut self, json: &UsageJson) {
        let mut usage = self
            .usage
            .unwrap_or_else(|| TokenUsage::new(EvidenceBasis::ProviderReported));
        usage.basis = EvidenceBasis::ProviderReported;
        usage.input = json.prompt_token_count.or(usage.input);
        usage.output = json.candidates_token_count.or(usage.output);
        usage.cached_input = json.cached_content_token_count.or(usage.cached_input);
        self.usage = Some(usage);
    }
}

impl Wiring for GoogleDecoder {
    fn feed_payload(
        &mut self,
        payload: &str,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        if matches!(self.status, Status::Idle) {
            self.status = Status::Streaming;
        }
        let payload = payload.trim();
        if payload == "[DONE]" {
            self.status = Status::Done;
            return Ok(Flow::Continue);
        }
        let chunk: Chunk = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Decode(error.to_string()))?;
        Ok(self.handle(&chunk, sink))
    }

    fn finish_stream(&mut self, _sink: &mut dyn ProviderSink) -> Result<Flow, ProviderError> {
        Ok(Flow::Continue)
    }

    fn has_emitted(&self) -> bool {
        self.announced
    }

    fn has_data(&self) -> bool {
        !matches!(self.status, Status::Idle)
    }

    fn is_cancelled(&self) -> bool {
        matches!(self.status, Status::Cancelled)
    }

    fn is_done(&self) -> bool {
        matches!(self.status, Status::Done)
    }

    fn final_outcome(&self) -> ProviderOutcome {
        let stop = if self.tool_calls {
            StopReason::ToolCalls
        } else {
            self.stop.clone().unwrap_or(StopReason::EndTurn)
        };
        ProviderOutcome {
            usage: self.usage,
            stop,
        }
    }
}

/// Mapeia o `finishReason` do dialeto.
fn map_stop(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::EndTurn,
        "MAX_TOKENS" => StopReason::Length,
        "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" => {
            StopReason::ContentFilter
        }
        other => StopReason::Other(other.to_string()),
    }
}
