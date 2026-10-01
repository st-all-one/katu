//! O provider declarativo: reusa os adaptadores de dialeto do built-in.

use katu_core::diag::{Level, events};
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
        let _span = katu_core::trace_fn!("declarative::provider::new");

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
        let _span = katu_core::trace_fn!("declarative::provider::from_json");

        Ok(Self::new(transport, ProviderSpec::from_json(json)?))
    }

    /// Fornece a chave de API (a borda lê-a do ambiente).
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        let _span = katu_core::trace_fn!("declarative::provider::with_api_key");

        self.api_key = Some(key.into());
        self
    }

    /// Substitui a política de retry.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        let _span = katu_core::trace_fn!("declarative::provider::with_retry");

        self.retry = retry;
        self
    }

    /// Definição em uso.
    #[must_use]
    pub fn spec(&self) -> &ProviderSpec {
        let _span = katu_core::trace_fn!("declarative::provider::spec");

        &self.spec
    }

    /// Pré-aquece a ligação (TCP/TLS) ao endpoint; ignora erros.
    pub fn warm(&self) {
        let _span = katu_core::trace_fn!("declarative::provider::warm");

        self.transport
            .warm(&engine::models_request(&self.wire(None)));
    }

    /// Vista de wire (para o despacho).
    fn wire<'a>(&'a self, entry: Option<&'a ModelEntry>) -> WireConfig<'a> {
        let _span = katu_core::trace_fn!("declarative::provider::wire");

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
            structured_output: self.spec.structured_output,
        }
    }
}

impl<T: Transport> Provider for Declarative<T> {
    fn id(&self) -> &str {
        let _span = katu_core::trace_fn!("declarative::provider::id");

        self.spec.name.as_str()
    }

    fn models(&self) -> Vec<String> {
        let _span = katu_core::trace_fn!("declarative::provider::models");

        self.catalog
            .models()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn dynamic_models(&self) -> Result<Vec<String>, ProviderError> {
        let _span = katu_core::fn_span!(
            Level::Debug,
            events::PROVIDER_MODELS,
            "declarative::provider::Declarative::dynamic_models"
        );
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
        let _span = katu_core::trace_fn!("declarative::provider::capabilities");

        ModelCapabilities {
            model: model.to_string(),
            reasoning: self
                .catalog
                .lookup(model)
                .is_some_and(|entry| entry.reasoning),
        }
    }

    fn model_for_tier(&self, tier: Tier) -> Option<String> {
        let _span = katu_core::trace_fn!("declarative::provider::model_for_tier");

        self.catalog.select_tier(tier).map(str::to_string)
    }

    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        let _span = katu_core::fn_span!(
            Level::Debug,
            events::PROVIDER_REQUEST,
            "declarative::provider::Declarative::stream"
        );
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
