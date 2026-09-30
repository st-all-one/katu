//! Testes do dialeto Google Gemini (encode + decode).

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Message};
use katu_core::provider::{
    CollectSink, ModelSpec, ProviderOutcome, ProviderRequest, StopReason, Thinking, TokenUsage,
    ToolDef,
};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use serde_json::json;

use crate::catalog::Dialect;
use crate::engine::{Call, WireConfig, endpoint_for};
use crate::openai::{EncodeOptions, Endpoint};
use crate::retry::RetryPolicy;
use crate::transport::MockTransport;

const TEXT_STREAM: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Ola\"}]}}]}\n\n",
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\" mundo\"}]}}]}\n\n",
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":2,\"cachedContentTokenCount\":1}}\n\n",
);

const CALL_STREAM: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"vejo\"},{\"functionCall\":{\"name\":\"bash\",\"args\":{\"argv\":[\"ls\"]}}}]}}]}\n\n",
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1}}\n\n",
);

/// Resultado de um decode: sink + desfecho.
type Decoded = (CollectSink, ProviderOutcome);

fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("seja breve".to_string()),
        messages: vec![Message::User {
            text: "oi".to_string(),
        }],
        tools: Vec::new(),
        max_tokens: Some(64),
        temperature: None,
    }
}

fn endpoint() -> Endpoint {
    Endpoint {
        url: "https://example.invalid/v1beta/models/gemini:streamGenerateContent?alt=sse"
            .to_string(),
        headers: Vec::new(),
    }
}

fn decode(stream: &str, request: &ProviderRequest) -> Result<Decoded, Box<dyn std::error::Error>> {
    let endpoint = endpoint();
    let options = EncodeOptions::default();
    let retry = RetryPolicy::disabled();
    let call = Call {
        endpoint: &endpoint,
        request,
        options: &options,
        retry: &retry,
    };
    let transport = MockTransport::ok(stream.to_string(), 7);
    let mut sink = CollectSink::default();
    let outcome = super::stream(&transport, &call, &mut sink)?;
    Ok((sink, outcome))
}

#[test]
fn endpoint_has_the_model_and_api_key() {
    let wire = WireConfig {
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        api_key: Some("k"),
        session: None,
        session_header: None,
        affinity_headers: &[],
        entry: None,
        reasoning_format: None,
        max_tokens: None,
        temperature: None,
    };
    let endpoint = endpoint_for(&wire, Dialect::Google, "gemini-2.5-pro");
    assert_eq!(
        endpoint.url,
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
    );
    assert!(
        endpoint
            .headers
            .iter()
            .any(|(key, value)| key == "x-goog-api-key" && value == "k")
    );
}

#[test]
fn encode_request_matches_generate_content() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = request("gemini-2.5-pro");
    request.model.thinking = Thinking::Low;
    request.tools = vec![ToolDef {
        name: "bash".to_string(),
        description: "corre um comando".to_string(),
        parameters: json!({"type": "object"}),
    }];
    request.temperature = Some(0.2);
    let body = super::encode::encode_request(&request, &EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(
        value.pointer("/systemInstruction/parts/0/text"),
        Some(&json!("seja breve"))
    );
    assert_eq!(value.pointer("/contents/0/role"), Some(&json!("user")));
    assert_eq!(
        value.pointer("/tools/0/functionDeclarations/0/name"),
        Some(&json!("bash"))
    );
    assert_eq!(
        value.pointer("/generationConfig/maxOutputTokens"),
        Some(&json!(64))
    );
    assert_eq!(
        value.pointer("/generationConfig/thinkingConfig/thinkingBudget"),
        Some(&json!(1024))
    );
    assert_eq!(
        value.pointer("/generationConfig/temperature"),
        Some(&json!(f64::from(0.2_f32)))
    );
    Ok(())
}

#[test]
fn decode_streams_text_and_usage() -> Result<(), Box<dyn std::error::Error>> {
    let request = request("gemini");
    let (sink, outcome) = decode(TEXT_STREAM, &request)?;
    assert_eq!(sink.text, "Ola mundo");
    assert_eq!(outcome.stop, StopReason::EndTurn);
    let usage: TokenUsage = outcome.usage.ok_or("sem usage")?;
    assert_eq!(usage.input, Some(3));
    assert_eq!(usage.output, Some(2));
    assert_eq!(usage.cached_input, Some(1));
    Ok(())
}

#[test]
fn decode_streams_a_function_call() -> Result<(), Box<dyn std::error::Error>> {
    let request = request("gemini");
    let (sink, outcome) = decode(CALL_STREAM, &request)?;
    assert_eq!(sink.text, "vejo");
    assert_eq!(outcome.stop, StopReason::ToolCalls);
    let Some((call, name, arguments)) = sink.calls.first() else {
        return Err("sem tool call".into());
    };
    assert_eq!(name, "bash");
    assert_eq!(arguments.get("argv"), Some(&json!(["ls"])));
    assert!(!call.as_str().is_empty());
    Ok(())
}

#[test]
fn tool_result_becomes_a_function_response() -> Result<(), Box<dyn std::error::Error>> {
    let path = ResolvedPath::from_canonical("/work/x.rs")?;
    let tool = ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: path.clone(),
            bytes: 3,
        },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    };
    let request = ProviderRequest {
        model: ModelSpec::new("gemini"),
        system: None,
        messages: vec![
            Message::ToolCall {
                call: CallId::new("c1"),
                tool,
            },
            Message::ToolResult {
                call: CallId::new("c1"),
                outcome: ToolOutcome::Ok,
            },
        ],
        tools: Vec::new(),
        max_tokens: None,
        temperature: None,
    };
    let body = super::encode::encode_request(&request, &EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(
        value.pointer("/contents/0/parts/0/functionCall/name"),
        Some(&json!("write"))
    );
    assert_eq!(
        value.pointer("/contents/1/parts/0/functionResponse/name"),
        Some(&json!("write"))
    );
    Ok(())
}
