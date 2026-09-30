//! Provider local **`llama.cpp`** via `llama-server` (E12-T08, L1).
//!
//! O `llama-server` fala o dialeto `chat/completions` (`OpenAI`-compatible) em
//! `http://127.0.0.1:<porta>/v1/chat/completions` e expõe `GET /health`. Reusa exatamente o
//! mesmo caminho do built-in: mesmo trait, mesma política, mesmo orçamento e benchmark. Sem
//! `unsafe` (a inferência in-process, L2, é outra tarefa e fica atrás de feature).

use katu_core::diag::{Level, events};
use katu_core::provider::{
    ModelCapabilities, Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
};

use super::catalog::Dialect;
use super::engine::{self, Dispatch, WireConfig};
use super::retry::RetryPolicy;
use super::transport::Transport;

/// Configuração do `llama-server` local.
#[derive(Debug, Clone)]
pub struct LlamaConfig {
    /// Base `OpenAI`-compatible (sem barra final), ex.: `http://127.0.0.1:8080/v1`.
    pub base_url: String,
    /// URL do `GET /health` (readiness).
    pub health_url: String,
    /// Teto de tokens por omissão.
    pub max_tokens: Option<u32>,
    /// Temperatura por omissão.
    pub temperature: Option<f32>,
    /// `reasoning_format` opcional (ex.: `"parsed"`).
    pub reasoning_format: Option<String>,
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
            reasoning_format: None,
        }
    }

    /// Fixa o `reasoning_format` pedido ao servidor.
    #[must_use]
    pub fn with_reasoning_format(mut self, format: impl Into<String>) -> Self {
        self.reasoning_format = Some(format.into());
        self
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

    /// Pré-aquece a ligação via `GET /health`; ignora erros (otimização de latência).
    pub fn warm(&self) {
        let _status = self.health();
    }
}

impl<T: Transport> Provider for Llama<T> {
    fn id(&self) -> &'static str {
        "llama"
    }

    fn capabilities(&self, model: &str) -> ModelCapabilities {
        ModelCapabilities {
            model: model.to_string(),
            // O llama.cpp aceita o grau de pensamento; o modelo local decide se o emite.
            reasoning: true,
        }
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
        let dispatch = Dispatch {
            wire: WireConfig {
                base_url: &self.config.base_url,
                api_key: None,
                session: None,
                session_header: None,
                affinity_headers: &[],
                entry: None,
                reasoning_format: self.config.reasoning_format.as_deref(),
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            },
            dialect: Dialect::ChatCompletions,
            request,
            retry: &self.retry,
        };
        engine::stream(&self.transport, &dispatch, sink)
    }
}
