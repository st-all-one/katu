//! e2e opcional (E12-T05): corre só com credenciais/URL no ambiente; auto-*skip* sem elas.
//!
//! - Built-in `opencode go/zen`: `KATU_OPENCODE_KEY` (+ `KATU_OPENCODE_BASE`, `KATU_OPENCODE_MODEL`).
//! - Local `llama-server`: `KATU_LLAMA_URL` (base `/v1`).
//!
//! Sem rede, ambos os testes devolvem `Ok(())` — o loop determinístico é coberto pelos testes do
//! crate com [`katu_providers::MockTransport`].

use std::time::Duration;

use katu_core::kernel::Message;
use katu_core::provider::{CollectSink, ModelSpec, Provider, ProviderRequest};
use katu_providers::{Dialect, Llama, LlamaConfig, OpenCode, OpenCodeConfig, UreqTransport};

/// Pedido mínimo com uma mensagem do utilizador.
fn request(model: &str, prompt: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("Responde em uma frase curta.".to_string()),
        messages: vec![Message::User {
            text: prompt.to_string(),
        }],
        tools: Vec::new(),
        max_tokens: Some(64),
        temperature: Some(0.0),
    }
}

/// Transporte real com timeouts de ligação/resposta.
fn transport() -> UreqTransport {
    UreqTransport::new(Duration::from_secs(5), Duration::from_secs(120))
}

#[test]
fn llama_server_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let Some(base) = std::env::var_os("KATU_LLAMA_URL") else {
        return Ok(());
    };
    let base = base.to_string_lossy().to_string();
    let health = base.trim_end_matches("/v1").to_string() + "/health";
    let config = LlamaConfig {
        base_url: base,
        health_url: health,
        max_tokens: Some(32),
        temperature: Some(0.0),
        reasoning_format: None,
        structured_output: false,
    };
    let provider = Llama::new(transport(), config);
    assert_eq!(provider.health()?, 200);

    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request("qwen", "Diga apenas: ola"), &mut sink)?;
    assert!(
        !sink.text.is_empty() || !sink.thinking.is_empty() || !sink.calls.is_empty(),
        "resposta vazia; outcome={outcome:?}"
    );
    Ok(())
}

#[test]
fn opencode_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let Some(key) = std::env::var_os("KATU_OPENCODE_KEY") else {
        return Ok(());
    };
    let key = key.to_string_lossy().to_string();
    let base = std::env::var_os("KATU_OPENCODE_BASE").map_or_else(
        || "https://opencode.ai/zen/go/v1".to_string(),
        |b| b.to_string_lossy().to_string(),
    );
    let model = std::env::var_os("KATU_OPENCODE_MODEL").map_or_else(
        || "longcat-2.5-preview-free".to_string(),
        |m| m.to_string_lossy().to_string(),
    );

    let config = OpenCodeConfig::at(base, key).with_session("katu-live-smoke");
    let provider = OpenCode::new(transport(), config);
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request(&model, "Diga apenas: ola"), &mut sink)?;
    assert!(
        !sink.text.is_empty() || !sink.thinking.is_empty() || !sink.calls.is_empty(),
        "resposta vazia; outcome={outcome:?}"
    );
    let models = provider.dynamic_models()?;
    assert!(!models.is_empty(), "catálogo do endpoint vazio (E12-T02)");
    Ok(())
}

/// Smoke de um dialeto do built-in (auto-*skip* sem chave).
fn opencode_dialect_smoke(
    dialect: Dialect,
    env_model: &str,
    default_model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(key) = std::env::var_os("KATU_OPENCODE_KEY") else {
        return Ok(());
    };
    let key = key.to_string_lossy().to_string();
    let base = std::env::var_os("KATU_OPENCODE_BASE").map_or_else(
        || "https://opencode.ai/zen/go/v1".to_string(),
        |b| b.to_string_lossy().to_string(),
    );
    let model = std::env::var_os(env_model).map_or_else(
        || default_model.to_string(),
        |m| m.to_string_lossy().to_string(),
    );
    let config = OpenCodeConfig::at(base, key)
        .with_session("katu-live-smoke")
        .with_dialect(dialect);
    let provider = OpenCode::new(transport(), config);
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request(&model, "Diga apenas: ola"), &mut sink)?;
    assert!(
        !sink.text.is_empty() || !sink.thinking.is_empty() || !sink.calls.is_empty(),
        "resposta vazia no dialeto {dialect:?}; outcome={outcome:?}"
    );
    Ok(())
}

#[test]
fn opencode_responses_smoke() -> Result<(), Box<dyn std::error::Error>> {
    opencode_dialect_smoke(
        Dialect::Responses,
        "KATU_OPENCODE_MODEL_RESPONSES",
        "gpt-5.5",
    )
}

#[test]
fn opencode_messages_smoke() -> Result<(), Box<dyn std::error::Error>> {
    opencode_dialect_smoke(
        Dialect::Messages,
        "KATU_OPENCODE_MODEL_MESSAGES",
        "claude-sonnet-4",
    )
}

#[test]
fn opencode_google_smoke() -> Result<(), Box<dyn std::error::Error>> {
    opencode_dialect_smoke(
        Dialect::Google,
        "KATU_OPENCODE_MODEL_GOOGLE",
        "gemini-2.5-pro",
    )
}
