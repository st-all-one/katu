//! Transporte HTTP bloqueante (E12-T06): a costura substituível do provider.
//!
//! O adaptador built-in é afinado para **latência**, não para compressão: sem `Accept-Encoding`
//! (identity), keep-alive/pooling e `TCP_NODELAY` no transporte real. O corpo é consumido **por
//! delta** ([`ChunkSink`]) — nunca bufferizado inteiro no hot path.
//!
//! O trait existe para que os adaptadores (`opencode`, `llama`) sejam testáveis sem rede: os
//! testes injetam um [`MockTransport`] que serve bytes canónicos.

use katu_core::provider::{Flow, ProviderError};

use crate::error::sanitize;

/// Método HTTP (só o que o endpoint de modelo precisa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `GET` (ex.: `/health`, `/models`).
    Get,
    /// `POST` (inferência).
    Post,
}

/// Cabeçalhos na ordem de envio.
pub type Headers = Vec<(String, String)>;

/// Pedido HTTP mínimo.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Método.
    pub method: Method,
    /// URL absoluta.
    pub url: String,
    /// Cabeçalhos na ordem de envio.
    pub headers: Headers,
    /// Corpo (ausente em `GET`).
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    /// Pedido `POST` com corpo JSON.
    #[must_use]
    pub fn post(
        url: impl Into<String>,
        body: impl Into<Vec<u8>>,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self {
            method: Method::Post,
            url: url.into(),
            headers,
            body: Some(body.into()),
        }
    }
}

/// Metadados da resposta; o corpo já foi entregue ao `sink`.
#[derive(Debug, Clone)]
pub struct HttpMeta {
    /// Código de estado.
    pub status: u16,
    /// Cabeçalhos da resposta.
    pub headers: Vec<(String, String)>,
    /// Bytes de corpo recebidos.
    pub bytes: u64,
}

/// Erro de transporte (mapeia para [`ProviderError`]).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// Falha de I/O.
    #[error("I/O no transporte: {0}")]
    Io(#[from] std::io::Error),
    /// Resposta/protocolo inválido.
    #[error("protocolo de transporte: {0}")]
    Protocol(String),
    /// Tempo esgotado.
    #[error("timeout do transporte ({millis} ms)")]
    Timeout {
        /// Limite em milissegundos.
        millis: u64,
    },
}

impl From<TransportError> for ProviderError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Timeout { millis } => Self::Timeout { millis },
            TransportError::Io(source) => Self::Transport(sanitize(&source.to_string())),
            TransportError::Protocol(message) => Self::Transport(sanitize(&message)),
        }
    }
}

/// Consumidor de bytes do corpo (deltas). Devolve [`Flow::Break`] para cancelar já.
pub type ChunkSink<'a> = dyn FnMut(&[u8]) -> Flow + 'a;

/// Teto de drenagem do [`Transport::warm`] (devolve a ligação ao *pool* sem ler tudo).
const WARM_DRAIN_LIMIT: usize = 16 * 1024;

/// Transporte substituível para um endpoint de modelo.
pub trait Transport: Send + Sync {
    /// Envia o pedido e alimenta `sink` com os bytes do corpo à medida que chegam.
    ///
    /// # Errors
    /// [`TransportError`] em falha de I/O, protocolo ou timeout.
    fn send(
        &self,
        request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError>;

    /// `GET` textual (health/models); conveniência sobre [`Transport::send`].
    ///
    /// # Errors
    /// [`TransportError`] em falha de I/O, protocolo ou timeout.
    fn get_text(&self, url: &str) -> Result<(u16, String), TransportError> {
        let request = HttpRequest {
            method: Method::Get,
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
        };
        let mut body = String::new();
        let meta = self.send(&request, &mut |chunk| {
            body.push_str(&String::from_utf8_lossy(chunk));
            Flow::Continue
        })?;
        Ok((meta.status, body))
    }

    /// Pré-aquece a ligação (TCP/TLS) ao endpoint, drenando o corpo até um teto.
    ///
    /// É uma **otimização**, não um contrato: erros são ignorados e o objetivo é deixar a
    /// ligação quente no *pool* antes do primeiro turno. O provider **não** toca o relógio.
    fn warm(&self, request: &HttpRequest) {
        let mut total = 0_usize;
        let _sent = self.send(request, &mut |chunk| {
            total = total.saturating_add(chunk.len());
            if total >= WARM_DRAIN_LIMIT {
                Flow::Break
            } else {
                Flow::Continue
            }
        });
    }
}

/// Transporte de teste: serve uma resposta canónica em memória.
#[derive(Debug, Clone)]
pub struct MockTransport {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Tamanho dos fragmentos entregues ao `sink` (para provar o consumo incremental).
    chunk: usize,
}

impl MockTransport {
    /// Resposta `200` com o corpo dado (entregue em fragmentos de `chunk` bytes).
    #[must_use]
    pub fn ok(body: impl Into<Vec<u8>>, chunk: usize) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_string(), "text/event-stream".to_string())],
            body: body.into(),
            chunk: chunk.max(1),
        }
    }

    /// Resposta com um estado explícito (para testar o caminho de erro HTTP).
    #[must_use]
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            chunk: 4096,
        }
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        _request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError> {
        let mut bytes = 0_u64;
        for fragment in self.body.chunks(self.chunk) {
            bytes = bytes.saturating_add(u64::try_from(fragment.len()).unwrap_or(u64::MAX));
            if matches!(sink(fragment), Flow::Break) {
                break;
            }
        }
        Ok(HttpMeta {
            status: self.status,
            headers: self.headers.clone(),
            bytes,
        })
    }
}
