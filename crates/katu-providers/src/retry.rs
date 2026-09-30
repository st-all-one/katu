//! Política de retry do provider (E12-T04): só antes do primeiro evento ao modelo.
//!
//! Repõe, no modelo bloqueante, a política dos SDKs: estados transitórios (`408`/`409`/`429`/`5xx`)
//! e falhas de transporte; respeita `x-should-retry` e `Retry-After`. Erros de conta/quota
//! (opencode `GoUsageLimitError`/`FreeTierError`, `insufficient_quota`, …) são **permanentes** —
//! repetir nunca ajudaria e só gastaria tempo. Um retry após qualquer delta emitido duplicaria
//! texto, por isso nunca acontece.

use std::time::Duration;

/// Política de retry (bounded; sem jitter — um cliente, não uma manada).
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Tentativas extra (0 = sem retry). A primeira chamada não conta.
    pub max_retries: u32,
    /// Atraso base; o atraso do `attempt` é `base * 2^attempt`, limitado a [`RetryPolicy::max_delay`].
    pub base_delay: Duration,
    /// Teto de qualquer atraso (inclui `Retry-After` do servidor).
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay: Duration::from_millis(400),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// Sem retry (diagnóstico/testes de um só disparo).
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }
}

/// Marcadores de erro **permanente** (repetir não ajuda): conta/quota/limites de subscrição.
const PERMANENT: &[&str] = &[
    "GoUsageLimitError",
    "FreeUsageLimitError",
    "FreeTierError",
    "Monthly usage limit reached",
    "available balance",
    "insufficient_quota",
    "quota exceeded",
    "out of budget",
    "billing",
];

/// Decide se uma resposta não-2xx merece retry.
#[must_use]
pub(crate) fn is_retryable(status: u16, headers: &[(String, String)], body: &str) -> bool {
    if let Some(value) = header(headers, "x-should-retry") {
        match value.trim() {
            "true" => return true,
            "false" => return false,
            _ => {}
        }
    }
    if PERMANENT.iter().any(|marker| body.contains(marker)) {
        return false;
    }
    matches!(status, 408 | 409 | 425 | 429) || status >= 500
}

/// Extrai um atraso pedido pelo servidor (`Retry-After`/`Retry-After-Ms`), já limitado.
#[must_use]
pub(crate) fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
    let millis = header(headers, "retry-after-ms").and_then(parse_secs);
    let seconds = header(headers, "retry-after").and_then(parse_secs);
    millis
        .map(|ms| Duration::from_secs_f64(ms / 1000.0))
        .or_else(|| seconds.map(Duration::from_secs_f64))
}

/// Atraso do `attempt` (o pedido do servidor vence a exponencial), limitado ao teto.
#[must_use]
pub(crate) fn delay(policy: &RetryPolicy, attempt: u32, requested: Option<Duration>) -> Duration {
    if let Some(server) = requested {
        return server.min(policy.max_delay);
    }
    let factor = 1_u32.checked_shl(attempt.min(16)).unwrap_or(u32::MAX);
    policy
        .base_delay
        .saturating_mul(factor)
        .min(policy.max_delay)
}

/// Lê um cabeçalho (case-insensitive).
fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Interpreta um valor decimal não-negativo.
fn parse_secs(value: &str) -> Option<f64> {
    let parsed = value.trim().parse::<f64>().ok()?;
    (parsed.is_finite() && parsed >= 0.0).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::{RetryPolicy, delay, is_retryable, retry_after};
    use std::time::Duration;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn transient_statuses_retry_but_account_limits_do_not() {
        assert!(is_retryable(429, &[], "rate limited"));
        assert!(is_retryable(503, &[], "overloaded"));
        assert!(!is_retryable(
            429,
            &[],
            r#"{"error":{"type":"FreeTierError"}}"#
        ));
        assert!(!is_retryable(400, &[], "bad request"));
        assert!(!is_retryable(
            500,
            &headers(&[("x-should-retry", "false")]),
            ""
        ));
        assert!(is_retryable(
            418,
            &headers(&[("x-should-retry", "true")]),
            ""
        ));
    }

    #[test]
    fn retry_after_prefers_millis_and_caps_at_the_policy_ceiling() {
        assert_eq!(
            retry_after(&headers(&[("Retry-After", "2")])),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            retry_after(&headers(&[("retry-after-ms", "1500")])),
            Some(Duration::from_millis(1500))
        );
        let policy = RetryPolicy {
            max_delay: Duration::from_secs(5),
            ..RetryPolicy::default()
        };
        assert_eq!(
            delay(&policy, 0, Some(Duration::from_secs(60))),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn exponential_backoff_is_bounded() {
        let policy = RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(250),
        };
        assert_eq!(delay(&policy, 0, None), Duration::from_millis(100));
        assert_eq!(delay(&policy, 1, None), Duration::from_millis(200));
        assert_eq!(delay(&policy, 2, None), Duration::from_millis(250));
    }
}
