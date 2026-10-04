//! Transporte real (ureq, bloqueante): pooling, `TCP_NODELAY` e sem compressão.
//!
//! O hot path não paga compressão de transporte nem re-encode: pedimos `identity` e lemos o corpo
//! por deltas. `connect` e `recv_response` têm timeout; o corpo tem um teto **total**
//! ([`DEFAULT_BODY_TIMEOUT`], G2) — o `timeout_recv_body` do `ureq` é total, não *idle*, e sem teto
//! uma leitura pendurada deixaria a thread de I/O viva para sempre.

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

/// Teto **total** do corpo (anti-fuga), por omissão.
///
/// O `timeout_recv_body` do `ureq` é **total**, não *idle* (L-P2): sem teto, uma leitura pendurada
/// deixa a thread de I/O viva para sempre. O *idle* por passo (60 s, `DEFAULT_IDLE_MS`) é imposto no
/// dreno; este teto é a rede de segurança do **transporte** — generoso para um stream saudável e
/// finito para um pendurado. É um limite de segurança, não um alvo de performance.
pub const DEFAULT_BODY_TIMEOUT: Duration = Duration::from_secs(600);

/// Transporte real sobre `ureq`.
pub struct UreqTransport {
    agent: Agent,
    recv_millis: u64,
    /// Teto total do corpo em milissegundos (relatado como timeout; G2).
    body_millis: u64,
    /// Comprime o corpo do `POST` (gzip) acima deste tamanho; `None` desliga.
    compress_above: Option<usize>,
}

impl UreqTransport {
    /// Constrói com os timeouts de ligação e de resposta e o teto total do corpo por omissão.
    #[must_use]
    pub fn new(connect: Duration, recv_response: Duration) -> Self {
        let _span = katu_core::trace_fn!("http::new");

        Self::with_body_timeout(connect, recv_response, DEFAULT_BODY_TIMEOUT)
    }

    /// Constrói com um teto total do corpo explícito (anti-fuga; ver [`DEFAULT_BODY_TIMEOUT`]).
    #[must_use]
    pub fn with_body_timeout(connect: Duration, recv_response: Duration, body: Duration) -> Self {
        let _span = katu_core::trace_fn!("http::with_body_timeout");

        let config = Self::config(connect, recv_response, body);
        Self {
            agent: Agent::new_with_config(config),
            recv_millis: u64::try_from(recv_response.as_millis()).unwrap_or(u64::MAX),
            body_millis: u64::try_from(body.as_millis()).unwrap_or(u64::MAX),
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
    fn config(connect: Duration, recv_response: Duration, body: Duration) -> Config {
        let _span = katu_core::trace_fn!("http::config");

        Agent::config_builder()
            .no_delay(true)
            .http_status_as_error(false)
            .max_idle_connections_per_host(4)
            .timeout_connect(Some(connect))
            .timeout_recv_response(Some(recv_response))
            // O corpo do stream pode ser longo; o teto **total** evita a fuga de thread (G2).
            .timeout_recv_body(Some(body))
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
            let read = reader.read(&mut buffer).map_err(|error| {
                if is_body_timeout(&error) {
                    TransportError::Timeout {
                        millis: self.body_millis,
                    }
                } else {
                    TransportError::Io(error)
                }
            })?;
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

/// `true` se o erro de leitura é o teto **total** do corpo do `ureq` (G2).
///
/// O `ureq` embrulha o timeout do corpo num [`std::io::Error`] (`kind: Other`), pelo que não chega
/// pelo `ureq::Error::Timeout` do [`map_error`]: é preciso desembrulhar o erro interno.
fn is_body_timeout(error: &std::io::Error) -> bool {
    let _span = katu_core::trace_fn!("http::is_body_timeout");

    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>())
        .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)))
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
    use crate::transport::{HttpRequest, Transport, TransportError};

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

    #[test]
    fn a_stalled_body_times_out_instead_of_leaking() -> Result<(), Box<dyn std::error::Error>> {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        use katu_core::provider::Flow;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Cabeçalhos + um chunk; depois nunca fecha nem envia o resto do corpo.
                let _written =
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nhello");
                let _flushed = stream.flush();
                // Drena o pedido e bloqueia até o cliente desistir (teto do corpo) e fechar.
                loop {
                    let mut buffer = [0_u8; 16];
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            }
        });
        let transport = UreqTransport::with_body_timeout(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_millis(200),
        );
        let request = HttpRequest::post(format!("http://{addr}/v1"), "{}", Vec::new());
        let mut sink = |_bytes: &[u8]| Flow::Continue;
        let result = transport.send(&request, &mut sink);
        assert!(
            matches!(result, Err(TransportError::Timeout { .. })),
            "corpo pendurado expira em vez de vazar: {result:?}"
        );
        server.join().ok();
        Ok(())
    }
}
