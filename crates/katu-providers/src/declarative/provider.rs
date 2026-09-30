//! O provider declarativo: reusa os adaptadores de dialeto do built-in.

use katu_core::provider::{
    ModelCapabilities, Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
    Tier,
};

use crate::catalog::{Catalog, ModelEntry};
use crate::engine::{self, Dispatch, WireConfig};
use crate::error::sanitize;
use crate::models::parse_models;
use crate::retry::RetryPolicy;
use crate::transport::Transport;

use super::{ProviderSpec, SpecError};

/// Provider genérico construído a partir de uma [`ProviderSpec`].
pub struct Declarative<T: Transport> {
    transport: T,
    spec: ProviderSpec,
    catalog: Catalog,
    api_key: Option<String>,
    retry: RetryPolicy,
}

impl<T: Transport> Declarative<T> {
    /// Constrói a partir de uma definição.
    #[must_use]
    pub fn new(transport: T, spec: ProviderSpec) -> Self {
        let catalog = spec.catalog();
        Self {
            transport,
            spec,
            catalog,
            api_key: None,
            retry: RetryPolicy::default(),
        }
    }

    /// Constrói a partir de um JSON.
    ///
    /// # Errors
    /// [`SpecError`] se o JSON for inválido.
    pub fn from_json(transport: T, json: &str) -> Result<Self, SpecError> {
        Ok(Self::new(transport, ProviderSpec::from_json(json)?))
    }

    /// Fornece a chave de API (a borda lê-a do ambiente).
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Substitui a política de retry.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Definição em uso.
    #[must_use]
    pub fn spec(&self) -> &ProviderSpec {
        &self.spec
    }

    /// Pré-aquece a ligação (TCP/TLS) ao endpoint; ignora erros.
    pub fn warm(&self) {
        self.transport
            .warm(&engine::models_request(&self.wire(None)));
    }

    /// Vista de wire (para o despacho).
    fn wire<'a>(&'a self, entry: Option<&'a ModelEntry>) -> WireConfig<'a> {
        WireConfig {
            base_url: &self.spec.base_url,
            api_key: self.api_key.as_deref(),
            session: None,
            session_header: self.spec.session_id_header.as_deref(),
            affinity_headers: &self.spec.affinity_headers,
            entry,
            reasoning_format: None,
            max_tokens: None,
            temperature: None,
        }
    }
}

impl<T: Transport> Provider for Declarative<T> {
    fn id(&self) -> &str {
        self.spec.name.as_str()
    }

    fn models(&self) -> Vec<String> {
        self.catalog
            .models()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn dynamic_models(&self) -> Result<Vec<String>, ProviderError> {
        let request = engine::models_request(&self.wire(None));
        let (status, body) = self.transport.get(&request).map_err(ProviderError::from)?;
        if !(200..300).contains(&status) {
            return Err(ProviderError::Http {
                status,
                body: sanitize(&body),
            });
        }
        let live = parse_models(&body);
        Ok(if live.is_empty() { self.models() } else { live })
    }

    fn capabilities(&self, model: &str) -> ModelCapabilities {
        ModelCapabilities {
            model: model.to_string(),
            reasoning: self
                .catalog
                .lookup(model)
                .is_some_and(|entry| entry.reasoning),
        }
    }

    fn model_for_tier(&self, tier: Tier) -> Option<String> {
        self.catalog.select_tier(tier).map(str::to_string)
    }

    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        let entry = self.catalog.lookup(&request.model.model);
        let dialect = entry.map_or_else(|| self.spec.engine.dialect(), |entry| entry.dialect);
        let dispatch = Dispatch {
            wire: self.wire(entry),
            dialect,
            request,
            retry: &self.retry,
        };
        engine::stream(&self.transport, &dispatch, sink)
    }
}
