//! Testes do dialeto Anthropic Messages (encode + decode).

use katu_core::provider::{CollectSink, ModelSpec, ProviderRequest, StopReason, TokenUsage};
use serde_json::json;

use crate::engine::Call;
use crate::openai::{EncodeOptions, Endpoint};
use crate::retry::RetryPolicy;
use crate::transport::MockTransport;

const STREAM: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"cache_read_input_tokens\":1}}}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Ola\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"bash\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"argv\\\":[\\\"ls\\\"]}\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":3}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("seja breve".to_string()),
        messages: Vec::new(),
        tools: Vec::new(),
        max_tokens: Some(64),
        temperature: None,
    }
}

#[test]
fn encode_request_matches_the_messages_api() -> Result<(), Box<dyn std::error::Error>> {
    let body = super::encode::encode_request(&request("claude"), &EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value.get("system"), Some(&json!("seja breve")));
    assert_eq!(value.get("max_tokens"), Some(&json!(64)));
    assert_eq!(value.get("stream"), Some(&json!(true)));
    Ok(())
}

#[test]
fn decode_streams_text_tool_calls_and_usage() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint {
        url: "https://example.invalid/v1/messages".to_string(),
        headers: Vec::new(),
    };
    let request = request("claude");
    let options = EncodeOptions::default();
    let retry = RetryPolicy::disabled();
    let call = Call {
        endpoint: &endpoint,
        request: &request,
        options: &options,
        retry: &retry,
    };
    let transport = MockTransport::ok(STREAM, 9);
    let mut sink = CollectSink::default();
    let outcome = super::stream(&transport, &call, &mut sink)?;
    assert_eq!(sink.text, "Ola");
    assert_eq!(outcome.stop, StopReason::ToolCalls);
    assert_eq!(sink.calls.len(), 1);
    let Some((call, name, arguments)) = sink.calls.first() else {
        return Err("sem tool call".into());
    };
    assert_eq!(call.as_str(), "toolu_1");
    assert_eq!(name, "bash");
    assert_eq!(arguments.get("argv"), Some(&json!(["ls"])));
    let usage: TokenUsage = outcome.usage.ok_or("sem usage")?;
    assert_eq!(usage.input, Some(5));
    assert_eq!(usage.output, Some(3));
    assert_eq!(usage.cached_input, Some(1));
    Ok(())
}
