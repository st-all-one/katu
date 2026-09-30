//! Driver de streaming SSE com retry (E12-T04/T06), genérico sobre o decodificador do dialeto.
//!
//! Uma só implementação da política de retry (só antes do primeiro evento), da captura do corpo
//! de erro e da contagem de chunks. Cada dialeto fornece um [`Wiring`] (o decodificador).

use std::thread::sleep;
use std::time::Duration;

use katu_core::diag::{Level, events};
use katu_core::provider::{Flow, ProviderError, ProviderOutcome, ProviderSink};

use crate::error;
use crate::retry::{self, RetryPolicy};
use crate::sse::SseParser;
use crate::transport::{HttpMeta, HttpRequest, Transport, TransportError};

/// Decodificador de um dialeto: o que o driver precisa para consumir o stream.
pub(crate) trait Wiring {
    /// Consome um payload SSE (`data:`).
    ///
    /// # Errors
    /// [`ProviderError::Decode`] se o payload não seguir o protocolo do dialeto.
    fn feed_payload(
        &mut self,
        payload: &str,
        sink: &mut dyn ProviderSink,
    ) -> Result<Flow, ProviderError>;

    /// Fecha o stream (emissão de tool calls pendentes).
    ///
    /// # Errors
    /// [`ProviderError::Decode`] se os argumentos acumulados forem inválidos.
    fn finish_stream(&mut self, sink: &mut dyn ProviderSink) -> Result<Flow, ProviderError>;

    /// `true` se já foi emitido um evento ao consumidor (um retry só é seguro antes).
    fn has_emitted(&self) -> bool;

    /// `true` se já chegou pelo menos um evento de dados.
    fn has_data(&self) -> bool;

    /// `true` se o sink cancelou o stream.
    fn is_cancelled(&self) -> bool;

    /// `true` se o stream terminou normalmente.
    fn is_done(&self) -> bool;

    /// Resultado final (após o fim do stream).
    fn final_outcome(&self) -> ProviderOutcome;
}

/// Executa um turno com retry seguro, criando um decodificador novo por tentativa.
pub(crate) fn stream<T, W, M>(
    transport: &T,
    http: &HttpRequest,
    retry: &RetryPolicy,
    mut make: M,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError>
where
    T: Transport,
    W: Wiring,
    M: FnMut() -> W,
{
    let mut attempt = 0_u32;
    loop {
        match run_attempt(transport, http, make(), sink) {
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

/// Decodifica argumentos de tool acumulados (string JSON; vazio = objeto vazio).
pub(crate) fn parse_arguments(raw: &str) -> Result<serde_json::Value, ProviderError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(trimmed).map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Falha de uma tentativa, com a decisão de retry já tomada.
struct Failure {
    error: ProviderError,
    retriable: bool,
    requested: Option<Duration>,
}

/// Executa **uma** tentativa (sem retry): envia, decodifica e classifica a falha.
fn run_attempt<T: Transport, W: Wiring>(
    transport: &T,
    http: &HttpRequest,
    mut decoder: W,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, Failure> {
    let attempt = send_and_feed(transport, http, &mut decoder, sink);
    let (result, failure, error_body) = (attempt.result, attempt.failure, attempt.error_body);
    let meta = match result {
        Ok(meta) => meta,
        Err(error) => {
            return Err(Failure {
                error: ProviderError::from(error),
                retriable: !decoder.has_emitted(),
                requested: None,
            });
        }
    };
    if meta.status >= 400 {
        return Err(http_failure(&meta, &error_body, &decoder));
    }
    if let Some(error) = failure {
        return Err(Failure {
            error,
            retriable: !decoder.has_emitted(),
            requested: None,
        });
    }
    decoder.finish_stream(sink).map_err(|error| Failure {
        error,
        retriable: false,
        requested: None,
    })?;
    if decoder.is_cancelled() && !decoder.is_done() {
        return Err(Failure {
            error: ProviderError::Cancelled,
            retriable: false,
            requested: None,
        });
    }
    Ok(decoder.final_outcome())
}

/// Classifica uma resposta HTTP não-2xx (corpo de erro, retry e atraso pedido).
fn http_failure<W: Wiring>(meta: &HttpMeta, body: &str, decoder: &W) -> Failure {
    Failure {
        error: ProviderError::Http {
            status: meta.status,
            body: error::normalize(body),
        },
        retriable: !decoder.has_emitted() && retry::is_retryable(meta.status, &meta.headers, body),
        requested: retry::retry_after(&meta.headers).or_else(|| retry::body_retry_after(body)),
    }
}

/// Resultado do envio de uma tentativa.
struct Attempt {
    result: Result<HttpMeta, TransportError>,
    failure: Option<ProviderError>,
    error_body: String,
}

/// Envia o pedido, alimenta o parser e o decodificador, e captura erro/`failure`.
fn send_and_feed<T: Transport, W: Wiring>(
    transport: &T,
    http: &HttpRequest,
    decoder: &mut W,
    sink: &mut dyn ProviderSink,
) -> Attempt {
    let mut parser = SseParser::new();
    let mut error_body = String::new();
    let mut failure = None;
    let result = transport.send(http, &mut |chunk| {
        katu_core::event!(Level::Trace, events::PROVIDER_CHUNK, "bytes" => chunk.len());
        if !decoder.has_data() && error_body.len() < 2048 {
            error_body.push_str(&String::from_utf8_lossy(chunk));
        }
        parser.push(
            chunk,
            &mut |payload| match decoder.feed_payload(payload, sink) {
                Ok(Flow::Break) => Flow::Break,
                Ok(Flow::Continue) => Flow::Continue,
                Err(error) => {
                    failure = Some(error);
                    Flow::Break
                }
            },
        )
    });
    Attempt {
        result,
        failure,
        error_body,
    }
}
