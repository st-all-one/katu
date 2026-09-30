//! Built-in **`opencode go/zen`** (E12-T01/T06): gateway stateless de modelo.
//!
//! Dois produtos, o mesmo seam:
//!
//! - **Zen** (pay-as-you-go): `https://opencode.ai/zen/v1/*`
//! - **Go** (subscrição): `https://opencode.ai/zen/go/v1/*` — exige `x-opencode-session`
//!   (afinidade de sessão/prefix-cache).
//!
//! O dialeto por omissão é `chat/completions`; o [`Catalog`] embutido (definição declarativa)
//! pode encaminhar um modelo para `responses`/`messages`. `google` fica explicitamente
//! `Unsupported`.

use katu_core::diag::{Level, events};
use katu_core::provider::{
    Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
};

use super::catalog::{Catalog, ModelEntry};
use super::declarative::ProviderSpec;
use super::engine::{self, Dispatch, WireConfig};
use super::openai::Endpoint;
use super::retry::RetryPolicy;
use super::transport::Transport;

pub use crate::catalog::Dialect;

/// Configuração do built-in `opencode`.
#[derive(Debug, Clone)]
pub struct OpenCodeConfig {
    /// Base do gateway (sem barra final), ex.: `https://opencode.ai/zen/go/v1`.
    pub base_url: String,
    /// Chave de API.
    pub api_key: String,
    /// Header `x-opencode-session` (afinidade; obrigatório no Go).
    pub session: Option<String>,
    /// Dialeto por omissão (o catálogo pode sobrepô-lo por modelo).
    pub dialect: Dialect,
    /// Teto de tokens por omissão.
    pub max_tokens: Option<u32>,
    /// Temperatura por omissão.
    pub temperature: Option<f32>,
    /// Nome do cabeçalho de afinidade.
    pub session_header: Option<String>,
    /// Cabeçalhos extra de afinidade (levam o id da sessão).
    pub affinity_headers: Vec<String>,
    /// Catálogo `model → dialeto` da definição declarativa.
    pub catalog: Catalog,
}

impl OpenCodeConfig {
    /// Zen (pay-as-you-go).
    #[must_use]
    pub fn zen(api_key: impl Into<String>) -> Self {
        Self::from_spec(&ProviderSpec::opencode_zen(), api_key)
    }

    /// Go (subscrição).
    #[must_use]
    pub fn go(api_key: impl Into<String>) -> Self {
        Self::from_spec(&ProviderSpec::opencode_go(), api_key)
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
            session_header: Some("x-opencode-session".to_string()),
            affinity_headers: Vec::new(),
            catalog: Catalog::new(),
        }
    }

    /// Constrói a partir de uma definição declarativa (a chave é injetada).
    fn from_spec(spec: &ProviderSpec, api_key: impl Into<String>) -> Self {
        Self {
            base_url: spec.base_url.clone(),
            api_key: api_key.into(),
            session: None,
            dialect: spec.engine.dialect(),
            max_tokens: None,
            temperature: None,
            session_header: spec.session_id_header.clone(),
            affinity_headers: spec.affinity_headers.clone(),
            catalog: spec.catalog(),
        }
    }

    /// Fixa a sessão de afinidade.
    #[must_use]
    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    /// Substitui a base do gateway.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Fixa o dialeto por omissão.
    #[must_use]
    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Substitui o catálogo.
    #[must_use]
    pub fn with_catalog(mut self, catalog: Catalog) -> Self {
        self.catalog = catalog;
        self
    }

    /// Endpoint + cabeçalhos do dialeto por omissão.
    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        engine::endpoint(&self.wire(None), Dialect::ChatCompletions)
    }

    /// Vista de wire (para o despacho).
    fn wire<'a>(&'a self, entry: Option<&'a ModelEntry>) -> WireConfig<'a> {
        WireConfig {
            base_url: &self.base_url,
            api_key: Some(self.api_key.as_str()),
            session: self.session.as_deref(),
            session_header: self.session_header.as_deref(),
            affinity_headers: &self.affinity_headers,
            entry,
            reasoning_format: None,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
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

    /// Fixa a sessão de afinidade (atalho).
    #[must_use]
    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.config = self.config.with_session(session);
        self
    }

    /// Configuração em uso.
    #[must_use]
    pub fn config(&self) -> &OpenCodeConfig {
        &self.config
    }

    /// Pré-aquece a ligação (TCP/TLS) ao gateway; ignora erros.
    ///
    /// É uma otimização de latência: a ligação quente no *pool* evita o *handshake* no 1.º turno.
    pub fn warm(&self) {
        self.transport
            .warm(&engine::models_request(&self.config.wire(None)));
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
        let entry = self.config.catalog.lookup(&request.model.model);
        let dialect = entry.map_or(self.config.dialect, |entry| entry.dialect);
        let dispatch = Dispatch {
            wire: self.config.wire(entry),
            dialect,
            request,
            retry: &self.retry,
        };
        engine::stream(&self.transport, &dispatch, sink)
    }
}
