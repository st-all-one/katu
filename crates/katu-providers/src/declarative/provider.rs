//! O provider declarativo: reusa os adaptadores de dialeto do built-in.

use katu_core::provider::{
    Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink,
};

use crate::catalog::{Catalog, ModelEntry};
use crate::engine::{self, Dispatch, WireConfig};
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
