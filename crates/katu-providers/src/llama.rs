//! Provider local **`llama.cpp`** via `llama-server` (E12-T08, L1).
//!
//! O `llama-server` fala o dialeto `chat/completions` (OpenAI-compatible) em
//! `http://127.0.0.1:<porta>/v1/chat/completions` e expõe `GET /health`. Reusa exatamente o
//! mesmo caminho do built-in: mesmo trait, mesma política, mesmo orçamento e benchmark. Sem
//! `unsafe` (a inferência in-process, L2, é outra tarefa e fica atrás de feature).

use katu_core::diag::{Level, events};
use katu_core::provider::{
    Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
};

use super::openai::{self, Endpoint};
use super::retry::RetryPolicy;
use super::transport::Transport;

/// Configuração do `llama-server` local.
#[derive(Debug, Clone)]
pub struct LlamaConfig {
    /// Base OpenAI-compatible (sem barra final), ex.: `http://127.0.0.1:8080/v1`.
    pub base_url: String,
    /// URL do `GET /health` (readiness).
    pub health_url: String,
    /// Teto de tokens por omissão.
    pub max_tokens: Option<u32>,
    /// Temperatura por omissão.
    pub temperature: Option<f32>,
}

impl LlamaConfig {
    /// Servidor local numa porta.
    #[must_use]
    pub fn local(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            health_url: format!("http://127.0.0.1:{port}/health"),
            max_tokens: None,
            temperature: None,
        }
    }

    /// Endpoint do dialeto (sem autenticação; o servidor é local).
    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            url: format!("{}/chat/completions", self.base_url.trim_end_matches('/')),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("accept".to_string(), "text/event-stream".to_string()),
                ("accept-encoding".to_string(), "identity".to_string()),
                (
                    "user-agent".to_string(),
                    format!("katu/{}", env!("CARGO_PKG_VERSION")),
                ),
            ],
        }
    }
}

/// Provider `llama-server` (transporte injetado).
pub struct Llama<T: Transport> {
    transport: T,
    config: LlamaConfig,
    retry: RetryPolicy,
}

impl<T: Transport> Llama<T> {
    /// Constrói com transporte e configuração (retry por omissão).
    #[must_use]
    pub fn new(transport: T, config: LlamaConfig) -> Self {
        Self {
            transport,
            config,
            retry: RetryPolicy::default(),
        }
    }

    /// Substitui a política de retry.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Readiness: `GET /health` (código HTTP; `200` = pronto).
    ///
    /// # Errors
    /// [`ProviderError`] se o servidor não responder.
    pub fn health(&self) -> Result<u16, ProviderError> {
        let (status, _body) = self
            .transport
            .get_text(&self.config.health_url)
            .map_err(ProviderError::from)?;
        Ok(status)
    }
}

impl<T: Transport> Provider for Llama<T> {
    fn id(&self) -> &'static str {
        "llama"
    }

    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        let _span = katu_core::span!(
            Level::Trace,
            events::PROVIDER_REQUEST,
            "provider" => "llama",
            "model" => request.model.model.as_str(),
        );
        let mut request = request.clone();
        if request.max_tokens.is_none() {
            request.max_tokens = self.config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = self.config.temperature;
        }
        openai::stream_chat(
            &self.transport,
            &self.config.endpoint(),
            &request,
            &self.retry,
            sink,
        )
    }
}
