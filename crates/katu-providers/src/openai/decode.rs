//! Decodificação incremental do stream `chat.completion.chunk`.

use std::collections::BTreeMap;

use katu_core::diag::{Level, events};
use katu_core::evidence::EvidenceBasis;
use katu_core::kernel::CallId;
use katu_core::provider::{
    Flow, ProviderError, ProviderEvent, ProviderOutcome, ProviderSink, StopReason, TokenUsage,
};
use serde_json::Value;

use super::chunk::{Chunk, ToolCallDelta, UsageJson, text_of};
use crate::wire::Wiring;

/// Cycle de vida da decodificação.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Ainda sem dados.
    #[default]
    Idle,
    /// A receber deltas.
    Streaming,
    /// Terminou com `[DONE]`.
    Done,
    /// Cancelado pelo `sink`.
    Cancelled,
}

/// Acumulador de uma tool call fragmentada (por índice).
#[derive(Debug, Default)]
struct ToolAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Estado da decodificação de um stream.
#[derive(Debug, Default)]
pub(crate) struct ChatDecoder {
    usage: Option<TokenUsage>,
    tools: BTreeMap<u32, ToolAccum>,
    stop: Option<StopReason>,
    status: Status,
    announced: bool,
}

impl ChatDecoder {
    /// Novo decodificador.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `true` se já chegou pelo menos um evento de dados.
    pub(crate) const fn saw_data(&self) -> bool {
        !matches!(self.status, Status::Idle)
    }

    /// `true` se já foi emitido um evento ao modelo (um retry só é seguro antes disto).
    pub(crate) const fn emitted(&self) -> bool {
        self.announced
    }

    /// `true` se o sink cancelou o stream.
    pub(crate) const fn cancelled(&self) -> bool {
        matches!(self.status, Status::Cancelled)
    }

    /// `true` se o stream terminou com `[DONE]`.
    pub(crate) const fn done(&self) -> bool {
        matches!(self.status, Status::Done)
    }

    /// Consome um payload SSE (`data:`), emitindo deltas no `sink`.
    ///
    /// # Errors
    /// [`ProviderError::Decode`] se o payload não for JSON válido do dialeto.
    pub(crate) fn on_payload(
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
            return self.flush_tools(sink);
        }
        let chunk: Chunk = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Decode(error.to_string()))?;
        if let Some(usage) = &chunk.usage {
            self.absorb_usage(usage);
        }
        for choice in &chunk.choices {
            if let Some(delta) = &choice.delta {
                if let Some(text) = delta.reasoning().map(str::to_string)
                    && matches!(
                        self.announce(sink, ProviderEvent::Thinking(text)),
                        Flow::Break
                    )
                {
                    self.status = Status::Cancelled;
                    return Ok(Flow::Break);
                }
                if let Some(text) = delta
                    .content
                    .as_ref()
                    .and_then(text_of)
                    .filter(|t| !t.is_empty())
                    && matches!(self.announce(sink, ProviderEvent::Text(text)), Flow::Break)
                {
                    self.status = Status::Cancelled;
                    return Ok(Flow::Break);
                }
                if let Some(calls) = &delta.tool_calls {
                    self.absorb_tools(calls);
                }
            }
            if let Some(reason) = &choice.finish_reason {
                self.stop = Some(map_stop(reason));
            }
        }
        Ok(Flow::Continue)
    }

    /// Emite as tool calls acumuladas (idempotente; chamado no fim do stream).
    ///
    /// # Errors
    /// [`ProviderError::Decode`] se os argumentos acumulados não forem JSON válido.
    pub(crate) fn flush_tools(
        &mut self,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        if self.tools.is_empty() {
            return Ok(Flow::Continue);
        }
        let pending = std::mem::take(&mut self.tools);
        for (index, tool) in pending {
            let arguments = parse_arguments(&tool.arguments)?;
            let call = CallId::new(if tool.id.is_empty() {
                format!("call_{index}")
            } else {
                tool.id
            });
            let event = ProviderEvent::ToolCall {
                call,
                name: tool.name,
                arguments,
            };
            if matches!(self.announce(sink, event), Flow::Break) {
                self.status = Status::Cancelled;
                return Ok(Flow::Break);
            }
        }
        Ok(Flow::Continue)
    }

    /// Emite o evento, marcando o **TTFT** na primeira ocorrência (E12-T07).
    fn announce(&mut self, sink: &mut dyn ProviderSink, event: ProviderEvent) -> Flow {
        if !self.announced {
            self.announced = true;
            katu_core::event!(Level::Debug, events::PROVIDER_TTFT);
        }
        sink.on_event(event)
    }

    /// Resultado final (após o fim do stream).
    pub(crate) fn outcome(&self) -> ProviderOutcome {
        ProviderOutcome {
            usage: self.usage,
            stop: self.stop.clone().unwrap_or(StopReason::EndTurn),
        }
    }

    /// Absorve o `usage` de um chunk (base `provider_reported`).
    fn absorb_usage(&mut self, json: &UsageJson) {
        let mut usage = self
            .usage
            .unwrap_or_else(|| TokenUsage::new(EvidenceBasis::ProviderReported));
        usage.basis = EvidenceBasis::ProviderReported;
        usage.input = json.prompt_tokens.or(usage.input);
        usage.output = json.completion_tokens.or(usage.output);
        usage.cached_input = json
            .prompt_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens)
            .or(json.prompt_cache_hit_tokens)
            .or(json.cached_tokens)
            .or(usage.cached_input);
        usage.reasoning = json
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens)
            .or(usage.reasoning);
        self.usage = Some(usage);
    }

    /// Acumula fragmentos de tool calls por índice.
    fn absorb_tools(&mut self, calls: &[ToolCallDelta]) {
        for call in calls {
            let entry = self.tools.entry(call.index).or_default();
            if let Some(id) = call.id.as_deref().filter(|id| !id.is_empty()) {
                entry.id = id.to_string();
            }
            if let Some(function) = &call.function {
                if let Some(name) = function.name.as_deref().filter(|name| !name.is_empty()) {
                    entry.name = name.to_string();
                }
                if let Some(arguments) = &function.arguments {
                    entry.arguments.push_str(arguments);
                }
            }
        }
    }
}

/// Decodifica os argumentos acumulados (string JSON; vazio = objeto vazio).
fn parse_arguments(raw: &str) -> Result<Value, ProviderError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(trimmed).map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Mapeia o `finish_reason` do dialeto.
fn map_stop(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "tool_calls" | "function_call" => StopReason::ToolCalls,
        "length" => StopReason::Length,
        "content_filter" => StopReason::ContentFilter,
        other => StopReason::Other(other.to_string()),
    }
}

impl Wiring for ChatDecoder {
    fn feed_payload(
        &mut self,
        payload: &str,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError> {
        self.on_payload(payload, sink)
    }

    fn finish_stream(&mut self, sink: &mut dyn ProviderSink) -> Result<Flow, ProviderError> {
        self.flush_tools(sink)
    }

    fn has_emitted(&self) -> bool {
        self.emitted()
    }

    fn has_data(&self) -> bool {
        self.saw_data()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled()
    }

    fn is_done(&self) -> bool {
        self.done()
    }

    fn final_outcome(&self) -> ProviderOutcome {
        self.outcome()
    }
}
