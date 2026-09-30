//! Dialeto **`OpenAI` `chat/completions`** (E12-T06): o caminho quente do built-in.
//!
//! Cobre `zen/v1/chat/completions`, `zen/go/v1/chat/completions` e o `llama-server`
//! (`/v1/chat/completions`), que partilham o mesmo dialeto. O pedido pede `stream: true` com
//! `include_usage`; a resposta é consumida **incrementalmente** (ver [`decode`]) e as tool calls
//! só são emitidas **completas** (acumuladas por índice).
//!
//! O transporte é injetado ([`Transport`]): os testes servem bytes canónicos sem rede. Falhas
//! transitórias **antes** do primeiro delta são repetidas segundo a [`RetryPolicy`]; a partir do
//! primeiro delta, um retry duplicaria texto e por isso nunca acontece.

use std::thread::sleep;
use std::time::Duration;

use katu_core::diag::{Level, events};
use katu_core::provider::{Flow, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink};

use super::retry::{self, RetryPolicy};
use super::sse::SseParser;
use super::transport::{HttpRequest, Transport};

mod chunk;
mod decode;
mod encode;

pub(crate) use decode::ChatDecoder;
pub(crate) use encode::encode_request;

/// Destino de um dialeto: URL + cabeçalhos fixos.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// URL absoluta do endpoint (`.../chat/completions`).
    pub url: String,
    /// Cabeçalhos na ordem de envio.
    pub headers: Vec<(String, String)>,
}

/// Executa um turno no dialeto chat/completions, emitindo deltas no `sink`.
///
/// # Errors
/// [`ProviderError`] em falha de transporte, HTTP não-2xx, decodificação ou cancelamento.
pub fn stream_chat<T: Transport>(
    transport: &T,
    endpoint: &Endpoint,
    request: &ProviderRequest,
    retry: &RetryPolicy,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError> {
    let body = encode_request(request)?;
    let http = HttpRequest::post(endpoint.url.clone(), body, endpoint.headers.clone());
    let mut attempt = 0_u32;
    loop {
        match run_attempt(transport, &http, sink) {
            Ok(outcome) => return Ok(outcome),
            Err(failure) => {
                if failure.retriable && attempt < retry.max_retries {
                    katu_core::event!(Level::Warn, events::PROVIDER_RETRY, "attempt" => u64::from(attempt));
                    sleep(retry::delay(retry, attempt, failure.requested));
                    attempt = attempt.saturating_add(1);
                    continue;
                }
                katu_core::event!(Level::Warn, events::PROVIDER_ERROR);
                return Err(failure.error);
            }
        }
    }
}

/// Estado de **uma** tentativa: parser SSE + decodificador + captura de erro/corpo.
struct Wire {
    parser: SseParser,
    chat: ChatDecoder,
    failure: Option<ProviderError>,
    error_body: String,
}

impl Wire {
    fn new() -> Self {
        Self {
            parser: SseParser::new(),
            chat: ChatDecoder::new(),
            failure: None,
            error_body: String::new(),
        }
    }

    /// Consome um fragmento de bytes, alimentando o parser e o decodificador.
    fn feed(&mut self, chunk: &[u8], sink: &mut dyn ProviderSink) -> Flow {
        katu_core::event!(Level::Trace, events::PROVIDER_CHUNK, "bytes" => chunk.len());
        if !self.chat.saw_data() && self.error_body.len() < 2048 {
            self.error_body.push_str(&String::from_utf8_lossy(chunk));
        }
        let parser = &mut self.parser;
        let chat = &mut self.chat;
        let failure = &mut self.failure;
        parser.push(chunk, &mut |payload| match chat.on_payload(payload, sink) {
            Ok(Flow::Break) => Flow::Break,
            Ok(Flow::Continue) => Flow::Continue,
            Err(error) => {
                *failure = Some(error);
                Flow::Break
            }
        })
    }
}

/// Falha de uma tentativa, com a decisão de retry já tomada.
struct Failure {
    error: ProviderError,
    retriable: bool,
    requested: Option<Duration>,
}

/// Executa **uma** tentativa (sem retry): envia, decodifica e classifica a falha.
fn run_attempt<T: Transport>(
    transport: &T,
    http: &HttpRequest,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, Failure> {
    let mut wire = Wire::new();
    let result = transport.send(http, &mut |chunk| wire.feed(chunk, sink));
    let meta = match result {
        Ok(meta) => meta,
        Err(error) => {
            return Err(Failure {
                error: ProviderError::from(error),
                retriable: !wire.chat.emitted(),
                requested: None,
            });
        }
    };
    if meta.status >= 400 {
        let retriable = !wire.chat.emitted()
            && retry::is_retryable(meta.status, &meta.headers, &wire.error_body);
        return Err(Failure {
            error: ProviderError::Http {
                status: meta.status,
                body: wire.error_body,
            },
            retriable,
            requested: retry::retry_after(&meta.headers),
        });
    }
    if let Some(error) = wire.failure {
        return Err(Failure {
            error,
            retriable: !wire.chat.emitted(),
            requested: None,
        });
    }
    wire.chat.flush_tools(sink).map_err(|error| Failure {
        error,
        retriable: false,
        requested: None,
    })?;
    if wire.chat.cancelled() && !wire.chat.done() {
        return Err(Failure {
            error: ProviderError::Cancelled,
            retriable: false,
            requested: None,
        });
    }
    Ok(wire.chat.outcome())
}

#[cfg(test)]
mod tests;
