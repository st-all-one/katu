//! Transporte real (ureq, bloqueante): pooling, `TCP_NODELAY` e sem compressão.
//!
//! O hot path não paga compressão de transporte nem re-encode: pedimos `identity` e lemos o corpo
//! por deltas. `connect` e `recv_response` têm timeout; o corpo pode ser longo (streaming) e é
//! cancelável pelo `sink`.

use std::io::Read;
use std::time::Duration;

use katu_core::provider::Flow;
use ureq::Agent;
use ureq::RequestBuilder;
use ureq::config::Config;

use super::transport::{ChunkSink, HttpMeta, HttpRequest, Method, Transport, TransportError};

/// Tamanho do buffer de leitura por iteração (reutilizado; sem alocação por chunk).
const READ_BUFFER: usize = 4096;

/// Transporte real sobre `ureq`.
pub struct UreqTransport {
    agent: Agent,
    recv_millis: u64,
}

impl UreqTransport {
    /// Constrói com os timeouts de ligação e de resposta (segundos do corpo ficam a cargo do sink).
    #[must_use]
    pub fn new(connect: Duration, recv_response: Duration) -> Self {
        let config = Self::config(connect, recv_response);
        Self {
            agent: Agent::new_with_config(config),
            recv_millis: u64::try_from(recv_response.as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// Configuração afinada para um endpoint de modelo.
    fn config(connect: Duration, recv_response: Duration) -> Config {
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
        let mut response = match request.method {
            Method::Get => apply(self.agent.get(&request.url), &request.headers).call(),
            Method::Post => apply(self.agent.post(&request.url), &request.headers)
                .send(request.body.as_deref().unwrap_or("")),
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
    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    builder
}

/// Mapeia o erro do ureq para o transporte.
fn map_error(error: ureq::Error, recv_millis: u64) -> TransportError {
    match error {
        ureq::Error::Timeout(_) => TransportError::Timeout {
            millis: recv_millis,
        },
        ureq::Error::Io(source) => TransportError::Io(source),
        other => TransportError::Protocol(other.to_string()),
    }
}
