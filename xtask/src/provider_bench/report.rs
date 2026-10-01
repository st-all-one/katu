//! Corpus canónico, pedido de teste e agregados de percentis (E12-T07).
//!
//! Pura computação, sem I/O: mantém o instrumento ([`super`]) sob o limite de 300 linhas e torna
//! o corpus e os percentis testáveis isoladamente.

use std::time::Duration;

use katu_core::kernel::Message;
use katu_core::provider::{ModelSpec, ProviderRequest, TokenUsage};
use serde_json::{Value, json};

/// Deltas de texto no corpus canónico.
pub(super) const DELTAS: usize = 512;

/// Nanos de uma `Duration` (saturando).
pub(super) fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Corpus SSE canónico: texto incremental + tool call + usage + `[DONE]`.
pub(super) fn corpus() -> String {
    let mut out = String::with_capacity(DELTAS.saturating_mul(96).saturating_add(512));
    for index in 0..DELTAS {
        out.push_str("data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"katu-");
        out.push_str(&index.to_string());
        out.push_str("-delta\"},\"finish_reason\":null}]}\n\n");
    }
    out.push_str(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"src/lib.rs\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
    );
    out.push_str(
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":512,\"prompt_tokens_details\":{\"cached_tokens\":800},\"completion_tokens_details\":{\"reasoning_tokens\":10}}}\n\n",
    );
    out.push_str("data: [DONE]\n\n");
    out
}

/// Pedido de teste (sem tools).
pub(super) fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("Responde de forma curta.".to_string()),
        messages: vec![Message::User {
            text: "ola".to_string(),
        }],
        tools: Vec::new(),
        max_tokens: Some(64),
        temperature: Some(0.0),
    }
}

/// Usage como JSON (ou `null`), com a razão de acerto de cache quando há base.
pub(super) fn usage_value(usage: Option<TokenUsage>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let ratio = match (usage.input, usage.cached_input) {
        (Some(input), Some(cached)) if input > 0 => Some(
            f64::from(u32::try_from(cached).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(input).unwrap_or(u32::MAX)),
        ),
        _ => None,
    };
    json!({
        "input": usage.input,
        "output": usage.output,
        "cached": usage.cached_input,
        "reasoning": usage.reasoning,
        "cache_hit_ratio": ratio,
        "basis": usage.basis.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DELTAS, corpus};

    #[test]
    fn corpus_is_well_formed_sse() {
        let body = corpus();
        assert!(body.ends_with("data: [DONE]\n\n"));
        assert_eq!(body.matches("data: ").count(), DELTAS.saturating_add(3));
    }
}
