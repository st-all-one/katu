//! Política de retry do provider (E12-T04): só antes do primeiro evento ao modelo.
//!
//! Repõe, no modelo bloqueante, a política dos SDKs: estados transitórios (`408`/`409`/`429`/`5xx`)
//! e falhas de transporte; respeita `x-should-retry` e `Retry-After` (segundos, milissegundos ou
//! data `HTTP`, interpretada face ao `Date` da resposta — o provider não toca relógio). Erros de
//! conta/quota (opencode `GoUsageLimitError`/`FreeTierError`, `insufficient_quota`, …) são
//! **permanentes**: repetir só gastaria tempo. Um retry após qualquer delta duplicaria texto, por
//! isso nunca acontece.

use std::time::Duration;

use katu_core::diag::{Level, events};

/// Teto absoluto de um atraso pedido pelo servidor (1 h): um `1e30` malformado degrada para
/// "sem dica", nunca congela o agente.
const MAX_DELAY: Duration = Duration::from_secs(3600);

/// Política de retry (bounded; sem jitter — um cliente, não uma manada).
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Tentativas extra (0 = sem retry). A primeira chamada não conta.
    pub max_retries: u32,
    /// Atraso base; o do `attempt` é `base * 2^attempt`, limitado a [`RetryPolicy::max_delay`].
    pub base_delay: Duration,
    /// Teto de qualquer atraso (inclui `Retry-After` do servidor).
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        let _span = katu_core::trace_fn!("retry::default");

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
    let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_RETRY, "retry::is_retryable");
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

/// Extrai um atraso pedido pelo servidor em cabeçalhos, já limitado a [`MAX_DELAY`].
///
/// Ordem: `Retry-After-Ms` → `Retry-After` (segundos) → `Retry-After` (data `HTTP`, descontando o
/// cabeçalho `Date` da mesma resposta). Datas sem `Date` são ignoradas (o provider é puro).
#[must_use]
pub(crate) fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
    let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_RETRY, "retry::retry_after");
    if let Some(millis) = header(headers, "retry-after-ms").and_then(parse_secs) {
        return secs_to_duration(millis / 1000.0);
    }
    let raw = header(headers, "retry-after")?;
    if let Some(seconds) = parse_secs(raw) {
        return secs_to_duration(seconds);
    }
    let date = header(headers, "date").and_then(parse_http_date)?;
    let target = parse_http_date(raw)?;
    let seconds = u64::try_from(target.saturating_sub(date)).unwrap_or(0);
    Some(Duration::from_secs(seconds).min(MAX_DELAY))
}

/// Extrai um atraso do **corpo** (`error.metadata.retry_after_seconds`, forma `OpenRouter`).
#[must_use]
pub(crate) fn body_retry_after(body: &str) -> Option<Duration> {
    let _span = katu_core::trace_fn!("retry::body_retry_after");

    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let seconds = value
        .get("error")?
        .get("metadata")?
        .get("retry_after_seconds")?
        .as_f64()?;
    secs_to_duration(seconds)
}

/// Atraso do `attempt` (o pedido do servidor vence a exponencial), limitado ao teto.
#[must_use]
pub(crate) fn delay(policy: &RetryPolicy, attempt: u32, requested: Option<Duration>) -> Duration {
    let _span = katu_core::fn_span!(Level::Trace, events::PROVIDER_RETRY, "retry::delay");
    if let Some(server) = requested {
        return server.min(policy.max_delay);
    }
    let factor = 1_u32.checked_shl(attempt.min(16)).unwrap_or(u32::MAX);
    policy
        .base_delay
        .saturating_mul(factor)
        .min(policy.max_delay)
}

/// Converte segundos (finitos, não-negativos e limitados) num [`Duration`].
fn secs_to_duration(seconds: f64) -> Option<Duration> {
    let _span = katu_core::trace_fn!("retry::secs_to_duration");

    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(Duration::from_secs_f64(
        seconds.min(MAX_DELAY.as_secs_f64()),
    ))
}

/// Interpreta uma data `IMF-fixdate` (`Sun, 06 Nov 1994 08:49:37 GMT`) em segundos `epoch`.
fn parse_http_date(value: &str) -> Option<i64> {
    let _span = katu_core::trace_fn!("retry::parse_http_date");

    let rest = value.split_once(", ").map_or(value, |(_, rest)| rest);
    let mut parts = rest.split_whitespace();
    let day = parts.next()?.parse::<i64>().ok()?;
    let month = month_index(parts.next()?)?;
    let year = parts.next()?.parse::<i64>().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour = clock.next()?.parse::<i64>().ok()?;
    let minute = clock.next()?.parse::<i64>().ok()?;
    let second = clock.next()?.trim_end_matches(" GMT").parse::<i64>().ok()?;
    let days = days_from_civil(year, month, day);
    Some(
        days.wrapping_mul(86_400)
            .wrapping_add(hour.wrapping_mul(3_600))
            .wrapping_add(minute.wrapping_mul(60))
            .wrapping_add(second),
    )
}

/// Índice do mês (1–12) a partir do nome abreviado em inglês.
fn month_index(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS
        .iter()
        .position(|month| month.eq_ignore_ascii_case(name))
        .map(|index| i64::try_from(index).unwrap_or(0).wrapping_add(1))
}

/// Dias desde `1970-01-01` (algoritmo de Howard Hinnant; `wrapping_*` porque as entradas são
/// datas válidas e nunca chegam perto dos limites de `i64`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let _span = katu_core::trace_fn!("retry::days_from_civil");

    let year = if month <= 2 {
        year.wrapping_sub(1)
    } else {
        year
    };
    let era = (if year >= 0 {
        year
    } else {
        year.wrapping_sub(399)
    })
    .wrapping_div(400);
    let yoe = year.wrapping_sub(era.wrapping_mul(400));
    let mp = month.wrapping_add(9).wrapping_rem(12);
    let doy = 153_i64
        .wrapping_mul(mp)
        .wrapping_add(2)
        .wrapping_div(5)
        .wrapping_add(day)
        .wrapping_sub(1);
    let doe = yoe
        .wrapping_mul(365)
        .wrapping_add(yoe.wrapping_div(4))
        .wrapping_sub(yoe.wrapping_div(100))
        .wrapping_add(doy);
    era.wrapping_mul(146_097)
        .wrapping_add(doe)
        .wrapping_sub(719_468)
}

/// Lê um cabeçalho (case-insensitive).
fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let _span = katu_core::trace_fn!("retry::header");

    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Interpreta um valor decimal não-negativo.
fn parse_secs(value: &str) -> Option<f64> {
    let _span = katu_core::trace_fn!("retry::parse_secs");

    let parsed = value.trim().parse::<f64>().ok()?;
    (parsed.is_finite() && parsed >= 0.0).then_some(parsed)
}

#[cfg(test)]
mod tests;
