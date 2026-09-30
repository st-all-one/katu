//! Built-in **`opencode go/zen`** (E12-T01/T06): gateway stateless de modelo.
//!
//! Dois produtos, o mesmo seam:
//!
//! - **Zen** (pay-as-you-go): `https://opencode.ai/zen/v1/*`
//! - **Go** (subscrição): `https://opencode.ai/zen/go/v1/*` — exige `x-opencode-session`
//!   (afinidade de sessão/prefix-cache).
//!
//! O dialeto implementado é `chat/completions` (OpenAI-compatible); os dialetos
//! `responses`/`messages`/`google` ficam explicitamente `Unsupported` até E12-T06 os cobrir.

use katu_core::diag::{Level, events};
use katu_core::provider::{
    Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
};

use super::openai::{self, Endpoint};
use super::retry::RetryPolicy;
use super::transport::Transport;

/// Dialeto do gateway (normalizado no trait [`Provider`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dialect {
    /// `chat/completions` (OpenAI-compatible) — implementado.
    ChatCompletions,
    /// `responses` (`OpenAI` Responses `API`) — por implementar.
    Responses,
    /// `messages` (Anthropic) — por implementar.
    Messages,
    /// `models/<id>` (Google) — por implementar.
    Google,
}

impl Dialect {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat/completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::Google => "google",
        }
    }
}

/// Configuração do built-in `opencode`.
#[derive(Debug, Clone)]
pub struct OpenCodeConfig {
    /// Base do gateway (sem barra final), ex.: `https://opencode.ai/zen/go/v1`.
    pub base_url: String,
    /// Chave de API.
    pub api_key: String,
    /// Header `x-opencode-session` (afinidade; obrigatório no Go).
    pub session: Option<String>,
    /// Dialeto.
    pub dialect: Dialect,
    /// Teto de tokens por omissão.
    pub max_tokens: Option<u32>,
    /// Temperatura por omissão.
    pub temperature: Option<f32>,
}

impl OpenCodeConfig {
    /// Zen (pay-as-you-go).
    #[must_use]
    pub fn zen(api_key: impl Into<String>) -> Self {
        Self::at("https://opencode.ai/zen/v1", api_key)
    }

    /// Go (subscrição).
    #[must_use]
    pub fn go(api_key: impl Into<String>) -> Self {
        Self::at("https://opencode.ai/zen/go/v1", api_key)
    }

    /// Gateway arbitrário (útil em testes/self-hosted).
    #[must_use]
    pub fn at(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            session: None,
            dialect: Dialect::ChatCompletions,
            max_tokens: None,
            temperature: None,
        }
    }

    /// Fixa a sessão de afinidade.
    #[must_use]
    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    /// Fixa o dialeto.
    #[must_use]
    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Endpoint + cabeçalhos do dialeto.
    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        let mut headers = vec![
            (
                "authorization".to_string(),
                format!("Bearer {}", self.api_key),
            ),
            ("content-type".to_string(), "application/json".to_string()),
            ("accept".to_string(), "text/event-stream".to_string()),
            // Latência primeiro: sem compressão de transporte.
            ("accept-encoding".to_string(), "identity".to_string()),
            (
                "user-agent".to_string(),
                format!("katu/{}", env!("CARGO_PKG_VERSION")),
            ),
        ];
        if let Some(session) = &self.session {
            headers.push(("x-opencode-session".to_string(), session.clone()));
        }
        Endpoint {
            url: format!("{}/chat/completions", self.base_url.trim_end_matches('/')),
            headers,
        }
    }
}

/// Provider built-in `opencode` (transporte injetado).
pub struct OpenCode<T: Transport> {
    transport: T,
    config: OpenCodeConfig,
    retry: RetryPolicy,
}

impl<T: Transport> OpenCode<T> {
    /// Constrói com transporte e configuração (retry por omissão).
    #[must_use]
    pub fn new(transport: T, config: OpenCodeConfig) -> Self {
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

    /// Configuração em uso.
    #[must_use]
    pub fn config(&self) -> &OpenCodeConfig {
        &self.config
    }
}

impl<T: Transport> Provider for OpenCode<T> {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        let _span = katu_core::span!(
            Level::Trace,
            events::PROVIDER_REQUEST,
            "provider" => "opencode",
            "model" => request.model.model.as_str(),
            "dialect" => self.config.dialect.as_str(),
        );
        let mut request = request.clone();
        if request.max_tokens.is_none() {
            request.max_tokens = self.config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = self.config.temperature;
        }
        match self.config.dialect {
            Dialect::ChatCompletions => openai::stream_chat(
                &self.transport,
                &self.config.endpoint(),
                &request,
                &self.retry,
                sink,
            ),
            other => Err(ProviderError::Unsupported(other.as_str().to_string())),
        }
    }
}
