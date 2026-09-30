//! Transporte real (ureq, bloqueante): pooling, `TCP_NODELAY` e sem compressão.
//!
//! O hot path não paga compressão de transporte nem re-encode: pedimos `identity` e lemos o corpo
//! por deltas. `connect` e `recv_response` têm timeout; o corpo pode ser longo (streaming) e é
//! cancelável pelo `sink`.

use std::io::Read;
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use katu_core::diag::{Level, events};
use katu_core::provider::Flow;
use ureq::Agent;
use ureq::RequestBuilder;
use ureq::config::Config;

use super::transport::{
    ChunkSink, Headers, HttpMeta, HttpRequest, Method, Transport, TransportError,
};

/// Tamanho do buffer de leitura por iteração (reutilizado; sem alocação por chunk).
const READ_BUFFER: usize = 4096;

/// Transporte real sobre `ureq`.
pub struct UreqTransport {
    agent: Agent,
    recv_millis: u64,
    /// Comprime o corpo do `POST` (gzip) acima deste tamanho; `None` desliga.
    compress_above: Option<usize>,
}

impl UreqTransport {
    /// Constrói com os timeouts de ligação e de resposta (segundos do corpo ficam a cargo do sink).
    #[must_use]
    pub fn new(connect: Duration, recv_response: Duration) -> Self {
        let _span = katu_core::trace_fn!("http::new");

        let config = Self::config(connect, recv_response);
        Self {
            agent: Agent::new_with_config(config),
            recv_millis: u64::try_from(recv_response.as_millis()).unwrap_or(u64::MAX),
            compress_above: None,
        }
    }

    /// Comprime o corpo do pedido (gzip nível rápido) quando excede `threshold` bytes.
    ///
    /// Latência primeiro: só vale a pena em corpos grandes (histórico + tools). Abaixo do limiar
    /// o corpo vai em claro, sem custo de CPU.
    #[must_use]
    pub fn with_request_compression(mut self, threshold: usize) -> Self {
        let _span = katu_core::trace_fn!("http::with_request_compression");

        self.compress_above = Some(threshold);
        self
    }

    /// Prepara `(corpo, headers)` aplicando gzip quando compensa.
    fn prepare(&self, request: &HttpRequest) -> (Vec<u8>, Headers) {
        let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_REQUEST, "http::prepare");
        let body = request.body.clone().unwrap_or_default();
        let Some(threshold) = self.compress_above else {
            return (body, request.headers.clone());
        };
        if body.len() < threshold {
            return (body, request.headers.clone());
        }
        match gzip(&body) {
            Some(compressed) if compressed.len() < body.len() => {
                let mut headers = request.headers.clone();
                headers.push(("content-encoding".to_string(), "gzip".to_string()));
                (compressed, headers)
            }
            _ => (body, request.headers.clone()),
        }
    }

    /// Configuração afinada para um endpoint de modelo.
    fn config(connect: Duration, recv_response: Duration) -> Config {
        let _span = katu_core::trace_fn!("http::config");

        Agent::config_builder()
            .no_delay(true)
            .http_status_as_error(false)
            .max_idle_connections_per_host(4)
            .timeout_connect(Some(connect))
            .timeout_recv_response(Some(recv_response))
            // O corpo do stream pode demorar: sem teto global; o consumidor cancela.
            .timeout_recv_body(None)
            .build()
    }
}

impl Transport for UreqTransport {
    fn send(
        &self,
        request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError> {
        let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_REQUEST, "http::send");
        let mut response = match request.method {
            Method::Get => apply(self.agent.get(&request.url), &request.headers).call(),
            Method::Post => {
                let (body, headers) = self.prepare(request);
                apply(self.agent.post(&request.url), &headers).send(body)
            }
        }
        .map_err(|error| map_error(error, self.recv_millis))?;

        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let mut bytes = 0_u64;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0_u8; READ_BUFFER];
        loop {
            let read = reader.read(&mut buffer).map_err(TransportError::Io)?;
            if read == 0 {
                break;
            }
            bytes = bytes.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
            if matches!(sink(buffer.get(..read).unwrap_or(&[])), Flow::Break) {
                break;
            }
        }
        Ok(HttpMeta {
            status,
            headers: response_headers,
            bytes,
        })
    }
}

/// Aplica os cabeçalhos a um pedido ureq (genérico no tipo-estado).
fn apply<B>(mut builder: RequestBuilder<B>, headers: &[(String, String)]) -> RequestBuilder<B> {
    let _span = katu_core::trace_fn!("http::apply");

    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    builder
}

/// Comprime com gzip (nível rápido: latência primeiro).
fn gzip(body: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(body).ok()?;
    encoder.finish().ok()
}

/// Mapeia o erro do ureq para o transporte.
fn map_error(error: ureq::Error, recv_millis: u64) -> TransportError {
    let _span = katu_core::trace_fn!("http::map_error");

    match error {
        ureq::Error::Timeout(_) => TransportError::Timeout {
            millis: recv_millis,
        },
        ureq::Error::Io(source) => TransportError::Io(source),
        other => TransportError::Protocol(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::time::Duration;

    use flate2::read::GzDecoder;

    use super::{UreqTransport, gzip};
    use crate::transport::HttpRequest;

    #[test]
    fn gzip_roundtrips_and_shrinks() -> Result<(), Box<dyn std::error::Error>> {
        let input = "katu ".repeat(2_000);
        let compressed = gzip(input.as_bytes()).ok_or("gzip falhou")?;
        assert!(compressed.len() < input.len());
        let mut decoder = GzDecoder::new(compressed.as_slice());
        let mut output = String::new();
        decoder.read_to_string(&mut output)?;
        assert_eq!(output, input);
        Ok(())
    }

    #[test]
    fn prepare_compresses_only_above_the_threshold() {
        let transport = UreqTransport::new(Duration::from_secs(1), Duration::from_secs(1))
            .with_request_compression(100);
        let small = HttpRequest::post("https://x.invalid", "pequeno", Vec::new());
        let (body, headers) = transport.prepare(&small);
        assert_eq!(body.as_slice(), b"pequeno");
        assert!(!headers.iter().any(|(key, _)| key == "content-encoding"));

        let big = HttpRequest::post("https://x.invalid", "katu ".repeat(2_000), Vec::new());
        let (body, headers) = transport.prepare(&big);
        assert!(body.len() < 10_000);
        assert!(
            headers
                .iter()
                .any(|(key, value)| key == "content-encoding" && value == "gzip")
        );
    }
}
