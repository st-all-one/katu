//! Testes do caminho declarativo (E12-T02).

use katu_core::provider::{CollectSink, ModelSpec, Provider, ProviderRequest};

use super::{Declarative, Engine, ProviderSpec};
use crate::catalog::{Dialect, MaxTokensField};
use crate::transport::MockTransport;

const STREAM: &str = concat!(
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"oi\"},\"finish_reason\":null}]}\n\n",
    "data: [DONE]\n\n",
);

fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: None,
        messages: Vec::new(),
        tools: Vec::new(),
        max_tokens: None,
        temperature: None,
    }
}

#[test]
fn embedded_specs_parse_and_default_the_dialect() {
    let zen = ProviderSpec::opencode_zen();
    assert_eq!(zen.name, "opencode_zen");
    assert_eq!(zen.engine, Engine::OpenAi);
    let catalog = zen.catalog();
    let kimi = catalog.lookup("kimi-k3");
    assert_eq!(
        kimi.map(|entry| entry.dialect),
        Some(Dialect::ChatCompletions)
    );
    assert_eq!(kimi.and_then(|entry| entry.context_limit), Some(262_144));

    assert_eq!(
        ProviderSpec::opencode_go().session_id_header.as_deref(),
        Some("x-opencode-session")
    );

    let openai = ProviderSpec::openai();
    assert_eq!(openai.engine, Engine::OpenAiResponses);
    let catalog = openai.catalog();
    let gpt = catalog.lookup("gpt-5.5");
    assert_eq!(gpt.map(|entry| entry.dialect), Some(Dialect::Responses));
    assert_eq!(
        gpt.map(|entry| entry.max_tokens_field),
        Some(MaxTokensField::MaxCompletionTokens)
    );
    assert!(gpt.is_some_and(|entry| entry.prompt_cache));
}

#[test]
fn per_model_dialect_overrides_the_engine() -> Result<(), Box<dyn std::error::Error>> {
    let spec = ProviderSpec::from_json(
        r#"{"name":"custom","engine":"openai","base_url":"https://example.invalid/v1",
            "models":[{"name":"a"},{"name":"b","dialect":"responses"}]}"#,
    )?;
    let catalog = spec.catalog();
    assert_eq!(
        catalog.lookup("a").map(|entry| entry.dialect),
        Some(Dialect::ChatCompletions)
    );
    assert_eq!(
        catalog.lookup("b").map(|entry| entry.dialect),
        Some(Dialect::Responses)
    );
    Ok(())
}

#[test]
fn invalid_json_is_rejected() {
    assert!(ProviderSpec::from_json("{").is_err());
}

#[test]
fn declarative_provider_streams_chat_completions() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Declarative::new(MockTransport::ok(STREAM, 5), ProviderSpec::opencode_zen())
        .with_api_key("k");
    assert_eq!(provider.id(), "opencode_zen");
    let mut sink = CollectSink::default();
    provider.stream(&request("kimi-k3"), &mut sink)?;
    assert_eq!(sink.text, "oi");
    Ok(())
}

#[test]
fn dynamic_models_read_the_endpoint_and_fall_back() -> Result<(), Box<dyn std::error::Error>> {
    let live = Declarative::new(
        MockTransport::ok(r#"{"data":[{"id":"z"},{"id":"y"}]}"#, 8),
        ProviderSpec::opencode_zen(),
    );
    assert_eq!(live.dynamic_models()?, vec!["y", "z"]);

    let empty = Declarative::new(MockTransport::ok("{}", 8), ProviderSpec::opencode_zen());
    assert_eq!(empty.dynamic_models()?, empty.models());
    Ok(())
}
