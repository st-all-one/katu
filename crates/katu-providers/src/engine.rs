//! Despacho por dialeto (E12-T06) e construção do endpoint.
//!
//! Um só lugar decide, a partir do [`Dialect`], qual o adaptador de wire a usar, e constrói o
//! [`Endpoint`] com a autenticação/afinidade certas. O built-in e os providers declarativos
//! partilham este caminho.

use katu_core::provider::{ProviderError, ProviderOutcome, ProviderRequest, ProviderSink};

use crate::anthropic;
use crate::catalog::{Dialect, MaxTokensField, ModelEntry};
use crate::openai::{self, EncodeOptions, Endpoint};
use crate::responses;
use crate::retry::RetryPolicy;
use crate::transport::{HttpRequest, Method, Transport};

/// Dados comuns para construir o endpoint e parametrizar o wire.
pub(crate) struct WireConfig<'a> {
    /// Base (sem barra final), ex.: `https://opencode.ai/zen/go/v1`.
    pub base_url: &'a str,
    /// Chave de API (a borda lê-a do ambiente).
    pub api_key: Option<&'a str>,
    /// Valor do cabeçalho de sessão.
    pub session: Option<&'a str>,
    /// Nome do cabeçalho de sessão.
    pub session_header: Option<&'a str>,
    /// Cabeçalhos extra que levam o id da sessão (afinidade; ex.: `x-session-affinity`).
    pub affinity_headers: &'a [String],
    /// Entrada de catálogo (parametriza o wire).
    pub entry: Option<&'a ModelEntry>,
    /// `reasoning_format` de fallback (quando a entrada não o traz).
    pub reasoning_format: Option<&'a str>,
    /// Teto de tokens por omissão do provider (usado se o pedido não trouxer).
    pub max_tokens: Option<u32>,
    /// Temperatura por omissão do provider (usada se o pedido não trouxer).
    pub temperature: Option<f32>,
}

/// Um turno já materializado (endpoint + opções + pedido + retry).
pub(crate) struct Call<'a> {
    /// Endpoint do dialeto.
    pub endpoint: &'a Endpoint,
    /// Pedido normalizado.
    pub request: &'a ProviderRequest,
    /// Opções de wire.
    pub options: &'a EncodeOptions,
    /// Política de retry.
    pub retry: &'a RetryPolicy,
}

/// Tudo o que o despacho precisa: wire + dialeto + pedido + retry.
pub(crate) struct Dispatch<'a> {
    /// Wire (base, autenticação, sessão, catálogo).
    pub wire: WireConfig<'a>,
    /// Dialeto efetivo.
    pub dialect: Dialect,
    /// Pedido normalizado.
    pub request: &'a ProviderRequest,
    /// Política de retry.
    pub retry: &'a RetryPolicy,
}

/// Despacha um turno para o adaptador do dialeto.
pub(crate) fn stream<T: Transport>(
    transport: &T,
    dispatch: &Dispatch<'_>,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError> {
    let endpoint = endpoint(&dispatch.wire, dispatch.dialect);
    let options = options(&dispatch.wire);
    let call = Call {
        endpoint: &endpoint,
        request: dispatch.request,
        options: &options,
        retry: dispatch.retry,
    };
    match dispatch.dialect {
        Dialect::ChatCompletions => openai::stream_chat(transport, &call, sink),
        Dialect::Responses => responses::stream(transport, &call, sink),
        Dialect::Messages => anthropic::stream(transport, &call, sink),
        Dialect::Google => Err(ProviderError::Unsupported("google".to_string())),
    }
}

/// Constrói o endpoint (URL + cabeçalhos) para o dialeto.
pub(crate) fn endpoint(wire: &WireConfig<'_>, dialect: Dialect) -> Endpoint {
    let base = wire.base_url.trim_end_matches('/');
    let path = match dialect {
        Dialect::ChatCompletions => "/chat/completions",
        Dialect::Responses => "/responses",
        Dialect::Messages => "/messages",
        Dialect::Google => "/models",
    };
    let mut headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        ("accept".to_string(), "text/event-stream".to_string()),
        // Latência primeiro: sem compressão de transporte.
        ("accept-encoding".to_string(), "identity".to_string()),
        (
            "user-agent".to_string(),
            format!("katu/{}", env!("CARGO_PKG_VERSION")),
        ),
    ];
    if let Some(key) = wire.api_key {
        push_auth(&mut headers, dialect, key);
    }
    if let (Some(header), Some(session)) = (wire.session_header, wire.session) {
        headers.push((header.to_string(), session.to_string()));
    }
    if let Some(session) = wire.session {
        for header in wire.affinity_headers {
            headers.push((header.clone(), session.to_string()));
        }
    }
    Endpoint {
        url: format!("{base}{path}"),
        headers,
    }
}

/// Opções de wire derivadas da entrada de catálogo.
pub(crate) fn options(wire: &WireConfig<'_>) -> EncodeOptions {
    let max_tokens_field = wire
        .entry
        .map_or(MaxTokensField::MaxTokens, |entry| entry.max_tokens_field);
    let prompt_cache = wire.entry.is_some_and(|entry| entry.prompt_cache);
    let cache_key = if prompt_cache {
        wire.session.map(str::to_string)
    } else {
        None
    };
    let reasoning_format = wire
        .entry
        .and_then(|entry| entry.reasoning_format.clone())
        .or_else(|| wire.reasoning_format.map(str::to_string));
    let prompt_cache_retention = if prompt_cache {
        wire.entry
            .and_then(|entry| entry.prompt_cache_retention.clone())
    } else {
        None
    };
    EncodeOptions {
        max_tokens_field,
        prompt_cache_retention,
        prompt_cache_key: cache_key,
        reasoning_format,
        default_max_tokens: wire.max_tokens,
        default_temperature: wire.temperature,
    }
}

/// Pedido `GET .../models` (barato) para pré-aquecer a ligação (TCP/TLS) ao gateway.
pub(crate) fn models_request(wire: &WireConfig<'_>) -> HttpRequest {
    let mut endpoint = endpoint(wire, Dialect::ChatCompletions);
    endpoint.url = format!("{}/models", wire.base_url.trim_end_matches('/'));
    HttpRequest {
        method: Method::Get,
        url: endpoint.url,
        headers: endpoint.headers,
        body: None,
    }
}

/// Acrescenta os cabeçalhos de autenticação que o dialeto espera.
fn push_auth(headers: &mut Vec<(String, String)>, dialect: Dialect, key: &str) {
    match dialect {
        Dialect::Messages => {
            headers.push(("x-api-key".to_string(), key.to_string()));
            headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        Dialect::Google => headers.push(("x-goog-api-key".to_string(), key.to_string())),
        Dialect::ChatCompletions | Dialect::Responses => {
            headers.push(("authorization".to_string(), format!("Bearer {key}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WireConfig, models_request, options};
    use crate::catalog::{Dialect, MaxTokensField, ModelEntry};
    use crate::transport::Method;

    #[test]
    fn options_follow_the_catalog_entry() {
        let entry = ModelEntry::new("m", Dialect::Responses)
            .with_max_tokens_field(MaxTokensField::MaxCompletionTokens)
            .with_prompt_cache_retention("24h");
        let wire = WireConfig {
            base_url: "https://example.invalid/v1",
            api_key: None,
            session: Some("s"),
            session_header: None,
            affinity_headers: &[],
            entry: Some(&entry),
            reasoning_format: Some("parsed"),
            max_tokens: Some(64),
            temperature: Some(0.2),
        };
        let options = options(&wire);
        assert_eq!(
            options.max_tokens_field,
            MaxTokensField::MaxCompletionTokens
        );
        assert_eq!(options.prompt_cache_key.as_deref(), Some("s"));
        assert_eq!(options.prompt_cache_retention.as_deref(), Some("24h"));
        assert_eq!(options.reasoning_format.as_deref(), Some("parsed"));
        assert_eq!(options.default_max_tokens, Some(64));
        assert_eq!(options.default_temperature, Some(0.2));
    }

    #[test]
    fn models_request_targets_the_gateway_models_endpoint() {
        let wire = WireConfig {
            base_url: "https://gateway.invalid/v1/",
            api_key: Some("k"),
            session: Some("s"),
            session_header: Some("x-opencode-session"),
            affinity_headers: &[],
            entry: None,
            reasoning_format: None,
            max_tokens: None,
            temperature: None,
        };
        let request = models_request(&wire);
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, "https://gateway.invalid/v1/models");
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "authorization" && value == "Bearer k")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "x-opencode-session" && value == "s")
        );
    }

    #[test]
    fn endpoint_adds_affinity_headers() {
        let affinity = vec!["x-session-affinity".to_string()];
        let wire = WireConfig {
            base_url: "https://gateway.invalid/v1",
            api_key: None,
            session: Some("s"),
            session_header: Some("x-opencode-session"),
            affinity_headers: &affinity,
            entry: None,
            reasoning_format: None,
            max_tokens: None,
            temperature: None,
        };
        let endpoint = super::endpoint(&wire, Dialect::ChatCompletions);
        assert_eq!(endpoint.url, "https://gateway.invalid/v1/chat/completions");
        for header in ["x-opencode-session", "x-session-affinity"] {
            assert!(
                endpoint
                    .headers
                    .iter()
                    .any(|(key, value)| key == header && value == "s"),
                "falta o cabeçalho {header}"
            );
        }
    }
}
