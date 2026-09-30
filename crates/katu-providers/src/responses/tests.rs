//! Testes do dialeto Responses (encode + decode).

use katu_core::provider::{CollectSink, ModelSpec, ProviderRequest, StopReason, TokenUsage};
use serde_json::json;

use crate::engine::Call;
use crate::openai::{EncodeOptions, Endpoint};
use crate::retry::RetryPolicy;
use crate::transport::MockTransport;

const STREAM: &str = concat!(
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Ol\"}\n\n",
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"a\"}\n\n",
    "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"bash\"}}\n\n",
    "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"{\\\"argv\\\":\"}\n\n",
    "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"[\\\"ls\\\"]}\"}\n\n",
    "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"bash\",\"arguments\":\"{\\\"argv\\\":[\\\"ls\\\"]}\"}}\n\n",
    "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":1},\"output_tokens_details\":{\"reasoning_tokens\":0}}}}\n\n",
    "data: [DONE]\n\n",
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
fn encode_request_matches_the_responses_api() -> Result<(), Box<dyn std::error::Error>> {
    let options = EncodeOptions {
        prompt_cache_key: Some("sess".to_string()),
        prompt_cache_retention: Some("24h".to_string()),
        ..EncodeOptions::default()
    };
    let body = super::encode::encode_request(&request("gpt-5.5"), &options)?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value.get("instructions"), Some(&json!("seja breve")));
    assert_eq!(value.get("max_output_tokens"), Some(&json!(64)));
    assert_eq!(value.get("store"), Some(&json!(false)));
    assert_eq!(value.get("prompt_cache_key"), Some(&json!("sess")));
    assert!(value.get("input").and_then(|v| v.as_array()).is_some());
    Ok(())
}

#[test]
fn decode_streams_text_tool_calls_and_usage() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint {
        url: "https://example.invalid/v1/responses".to_string(),
        headers: Vec::new(),
    };
    let request = request("gpt-5.5");
    let options = EncodeOptions::default();
    let retry = RetryPolicy::disabled();
    let call = Call {
        endpoint: &endpoint,
        request: &request,
        options: &options,
        retry: &retry,
    };
    let transport = MockTransport::ok(STREAM, 7);
    let mut sink = CollectSink::default();
    let outcome = super::stream(&transport, &call, &mut sink)?;
    assert_eq!(sink.text, "Ola");
    assert_eq!(outcome.stop, StopReason::ToolCalls);
    assert_eq!(sink.calls.len(), 1);
    let Some((call, name, arguments)) = sink.calls.first() else {
        return Err("sem tool call".into());
    };
    assert_eq!(call.as_str(), "call_1");
    assert_eq!(name, "bash");
    assert_eq!(arguments.get("argv"), Some(&json!(["ls"])));
    let usage: TokenUsage = outcome.usage.ok_or("sem usage")?;
    assert_eq!(usage.input, Some(5));
    assert_eq!(usage.output, Some(3));
    assert_eq!(usage.cached_input, Some(1));
    Ok(())
}
