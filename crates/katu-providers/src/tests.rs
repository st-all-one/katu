//! Testes dos adaptadores sem rede: servem SSE canónico via [`MockTransport`].

use katu_core::provider::{
    CollectSink, Flow, ModelSpec, Provider, ProviderError, ProviderEvent, ProviderRequest,
    ProviderSink, StopReason, TokenUsage,
};
use serde_json::json;

use crate::fake::{FakeProvider, Turn};
use crate::llama::{Llama, LlamaConfig};
use crate::opencode::{Dialect, OpenCode, OpenCodeConfig};
use crate::transport::MockTransport;

mod retry;

/// Stream de texto do dialeto Google (`streamGenerateContent`).
const GOOGLE_STREAM: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Ola\"}]}}]}\n\n",
    "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2}}\n\n",
);

/// Pedido mínimo de teste.
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

/// Stream de texto com usage.
const TEXT_STREAM: &str = concat!(
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Ol\"},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"a\"},\"finish_reason\":null}],",
    "\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7,",
    "\"prompt_tokens_details\":{\"cached_tokens\":1},\"completion_tokens_details\":{\"reasoning_tokens\":0}}}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// Stream com tool call fragmentada.
const TOOL_STREAM: &str = concat!(
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",",
    "\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"argv\\\":\"}}]},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"[\\\"ls\\\"]}\"}}]},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
    "data: [DONE]\n\n",
);

#[test]
fn opencode_chat_streams_text_and_usage() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockTransport::ok(TEXT_STREAM, 7);
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    );
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request("m"), &mut sink)?;

    assert_eq!(sink.text, "Ola");
    assert_eq!(outcome.stop, StopReason::EndTurn);
    let usage: TokenUsage = outcome.usage.ok_or("sem usage")?;
    assert_eq!(usage.input, Some(5));
    assert_eq!(usage.output, Some(2));
    assert_eq!(usage.cached_input, Some(1));
    Ok(())
}

#[test]
fn opencode_emits_complete_tool_calls() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockTransport::ok(TOOL_STREAM, 13);
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    );
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request("m"), &mut sink)?;

    assert_eq!(outcome.stop, StopReason::ToolCalls);
    assert_eq!(sink.calls.len(), 1);
    let Some((call, name, arguments)) = sink.calls.first() else {
        return Err("sem tool call".into());
    };
    assert_eq!(call.as_str(), "call_1");
    assert_eq!(name, "bash");
    assert_eq!(
        arguments.get("argv").and_then(|v| v.get(0)),
        Some(&json!("ls"))
    );
    Ok(())
}

#[test]
fn opencode_reports_http_error_status() {
    let transport = MockTransport::status(402, br#"{"error":{"message":"sem fundos"}}"#.to_vec());
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    );
    let mut sink = CollectSink::default();
    let result = provider.stream(&request("m"), &mut sink);
    assert!(matches!(
        result,
        Err(ProviderError::Http { status: 402, .. })
    ));
}

#[test]
fn opencode_go_sets_session_header() {
    let config = OpenCodeConfig::go("k").with_session("s-1");
    let endpoint = config.endpoint();
    assert!(
        endpoint
            .headers
            .iter()
            .any(|(key, value)| key == "x-opencode-session" && value == "s-1")
    );
    assert!(endpoint.url.ends_with("/chat/completions"));
}

#[test]
fn google_dialect_streams_through_the_generate_content_path()
-> Result<(), Box<dyn std::error::Error>> {
    let config =
        OpenCodeConfig::at("https://example.invalid/v1beta", "k").with_dialect(Dialect::Google);
    let transport = MockTransport::ok(GOOGLE_STREAM, 5);
    let provider = OpenCode::new(transport, config);
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request("gemini-2.5-pro"), &mut sink)?;
    assert_eq!(sink.text, "Ola");
    assert_eq!(outcome.stop, StopReason::EndTurn);
    let usage: TokenUsage = outcome.usage.ok_or("sem usage")?;
    assert_eq!(usage.input, Some(5));
    Ok(())
}

#[test]
fn llama_health_and_chat_share_the_openai_path() -> Result<(), Box<dyn std::error::Error>> {
    let transport = MockTransport::ok(TEXT_STREAM, 5);
    let provider = Llama::new(transport, LlamaConfig::local(8080));
    assert_eq!(provider.health()?, 200);
    let mut sink = CollectSink::default();
    provider.stream(&request("qwen"), &mut sink)?;
    assert_eq!(sink.text, "Ola");
    Ok(())
}

#[test]
fn fake_replays_turns_then_exhausts() -> Result<(), Box<dyn std::error::Error>> {
    let provider = FakeProvider::new("fake", vec![Turn::text("um"), Turn::text("dois")]);
    let mut first = CollectSink::default();
    provider.stream(&request("m"), &mut first)?;
    assert_eq!(first.text, "um");
    let mut second = CollectSink::default();
    provider.stream(&request("m"), &mut second)?;
    assert_eq!(second.text, "dois");
    let mut third = CollectSink::default();
    assert!(provider.stream(&request("m"), &mut third).is_err());
    Ok(())
}

#[test]
fn thinking_deltas_are_separate_from_text() -> Result<(), Box<dyn std::error::Error>> {
    struct Probe {
        thinking: String,
        flow: Flow,
    }
    impl ProviderSink for Probe {
        fn on_event(&mut self, event: ProviderEvent) -> Flow {
            if let ProviderEvent::Thinking(delta) = event {
                self.thinking.push_str(&delta);
            }
            self.flow
        }
    }
    let transport = MockTransport::ok(
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"penso\"}}]}\n\ndata: [DONE]\n\n",
        6,
    );
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    );
    let mut probe = Probe {
        thinking: String::new(),
        flow: Flow::Continue,
    };
    provider.stream(&request("m"), &mut probe)?;
    assert_eq!(probe.thinking, "penso");
    Ok(())
}
