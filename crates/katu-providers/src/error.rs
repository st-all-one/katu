//! Normalização de erros do provider (E12-T06): mensagem útil e `URL`s sem segredos.
//!
//! O corpo de erro de um gateway não é estável: o `OpenAI` usa
//! `{"error":{"message":…}}`, outros usam `{"message":…}` ou `{"detail":…}`. Antes de guardar
//! (log/modelo), extrai-se a mensagem e removem-se credenciais e *query* de qualquer `URL` — o
//! texto do erro vai para o log e para o modelo.

use katu_core::diag::{Level, events};
use serde_json::Value;

/// Limite do resumo guardado (evita despejar corpos grandes no log).
const MAX_CHARS: usize = 512;

/// Extrai a mensagem útil de um corpo de erro (JSON ou texto) e sanitiza-a.
#[must_use]
pub(crate) fn normalize(body: &str) -> String {
    let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_ERROR, "error::normalize");
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let message = extract(trimmed).unwrap_or_else(|| truncate(trimmed));
    sanitize(&message)
}

/// Retira credenciais (`user:pass@`) e *query*/*fragment* de qualquer `URL` `http(s)`.
#[must_use]
pub(crate) fn sanitize(text: &str) -> String {
    let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_ERROR, "error::sanitize");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = scheme_at(rest) {
        let Some((head, tail)) = rest.split_at_checked(index) else {
            break;
        };
        out.push_str(head);
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '(' | ')' | '<' | '>' | ',')
            })
            .unwrap_or(tail.len());
        let Some((url, after)) = tail.split_at_checked(end) else {
            break;
        };
        out.push_str(&scrub(url));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Extrai `error.message`, `error` (string), `message` ou `detail` de um JSON.
fn extract(body: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("error::extract");

    let value: Value = serde_json::from_str(body).ok()?;
    let message = value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(Value::as_str))
        .or_else(|| value.get("detail").and_then(Value::as_str))?;
    Some(truncate(message))
}

/// Trunca por caracteres (não por bytes), com elipse.
fn truncate(text: &str) -> String {
    let _span = katu_core::trace_fn!("error::truncate");

    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MAX_CHARS).collect();
    out.push('…');
    out
}

/// Índice do primeiro esquema `http(s)://`.
fn scheme_at(text: &str) -> Option<usize> {
    let _span = katu_core::trace_fn!("error::scheme_at");

    match (text.find("http://"), text.find("https://")) {
        (Some(http), Some(https)) => Some(http.min(https)),
        (Some(http), None) => Some(http),
        (None, Some(https)) => Some(https),
        (None, None) => None,
    }
}

/// Remove `userinfo` e *query*/*fragment* de uma `URL`, preservando esquema/host/caminho.
fn scrub(url: &str) -> String {
    let _span = katu_core::trace_fn!("error::scrub");

    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = rest
        .find(['/', '?', '#'])
        .map_or((rest, ""), |index| rest.split_at(index));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let path = path.split(['?', '#']).next().unwrap_or("");
    format!("{scheme}://{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::{normalize, sanitize};

    #[test]
    fn extracts_openai_and_flat_error_shapes() {
        assert_eq!(
            normalize(r#"{"error":{"message":"rate limited"}}"#),
            "rate limited"
        );
        assert_eq!(normalize(r#"{"message":"nope"}"#), "nope");
        assert_eq!(normalize(r#"{"detail":"boom"}"#), "boom");
        assert_eq!(normalize("plain text"), "plain text");
    }

    #[test]
    fn strips_credentials_and_query_from_urls() {
        assert_eq!(
            sanitize("boom https://user:secret@api.example.com/v1?key=sk-123#frag end"),
            "boom https://api.example.com/v1 end"
        );
        assert_eq!(
            sanitize("see http://127.0.0.1:8080/health"),
            "see http://127.0.0.1:8080/health"
        );
    }
}
