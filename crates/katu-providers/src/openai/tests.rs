//! Testes da codificação do dialeto `chat/completions`.

use katu_core::provider::{ModelSpec, ProviderRequest};
use serde_json::json;

use crate::catalog::MaxTokensField;

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

#[test]
fn encode_request_is_openai_compatible() -> Result<(), Box<dyn std::error::Error>> {
    let body = super::encode_request(&request("m"), &super::EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value.get("stream"), Some(&json!(true)));
    assert_eq!(
        value
            .get("stream_options")
            .and_then(|v| v.get("include_usage")),
        Some(&json!(true))
    );
    assert_eq!(value.get("model"), Some(&json!("m")));
    let messages = value
        .get("messages")
        .and_then(|v| v.as_array())
        .ok_or("sem messages")?;
    assert_eq!(messages.len(), 1);
    Ok(())
}

#[test]
fn encode_request_applies_wire_options() -> Result<(), Box<dyn std::error::Error>> {
    let options = super::EncodeOptions {
        max_tokens_field: MaxTokensField::MaxCompletionTokens,
        prompt_cache_key: Some("sess".to_string()),
        prompt_cache_retention: Some("24h".to_string()),
        reasoning_format: Some("parsed".to_string()),
        ..super::EncodeOptions::default()
    };
    let body = super::encode_request(&request("m"), &options)?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value.get("max_completion_tokens"), Some(&json!(64)));
    assert!(value.get("max_tokens").is_none());
    assert_eq!(value.get("prompt_cache_key"), Some(&json!("sess")));
    assert_eq!(value.get("prompt_cache_retention"), Some(&json!("24h")));
    assert_eq!(value.get("reasoning_format"), Some(&json!("parsed")));
    Ok(())
}

#[test]
fn encode_request_uses_provider_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = request("m");
    request.max_tokens = None;
    request.temperature = Some(0.5);
    let options = super::EncodeOptions {
        default_max_tokens: Some(128),
        default_temperature: Some(0.9),
        ..super::EncodeOptions::default()
    };
    let body = super::encode_request(&request, &options)?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value.get("max_tokens"), Some(&json!(128)));
    // O valor do pedido tem prioridade sobre o default do provider.
    assert_eq!(value.get("temperature"), Some(&json!(0.5)));
    Ok(())
}

#[test]
fn tool_call_message_has_null_content() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    use katu_core::kernel::{CallId, Message};
    use katu_policy::{ResolvedPath, SearchMode, ToolArgs, ToolName, ToolUse};

    let cwd = ResolvedPath::from_canonical(PathBuf::from("/work"))?;
    let tool = ToolUse {
        name: ToolName::Search,
        args: ToolArgs::Search {
            root: cwd.clone(),
            query: "x".to_string(),
            mode: SearchMode::Find,
        },
        resolved_paths: Vec::new(),
        argv: None,
        cwd,
    };
    let mut request = request("m");
    request.system = None;
    request.messages = vec![Message::ToolCall {
        call: CallId::new("call_1"),
        tool,
    }];
    let body = super::encode_request(&request, &super::EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let message = value
        .get("messages")
        .and_then(|value| value.as_array())
        .and_then(|messages| messages.first())
        .ok_or("sem mensagem")?;
    assert_eq!(message.get("role"), Some(&json!("assistant")));
    assert_eq!(message.get("content"), Some(&serde_json::Value::Null));
    assert_eq!(
        message
            .get("tool_calls")
            .and_then(|calls| calls.as_array())
            .and_then(|calls| calls.first())
            .and_then(|call| call.get("function"))
            .and_then(|function| function.get("name")),
        Some(&json!("find"))
    );
    Ok(())
}

/// Conteúdo da mensagem de resultado de tool no corpo codificado.
fn tool_content(body: &serde_json::Value) -> Option<&str> {
    body.get("messages")
        .and_then(|value| value.as_array())
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.get("role") == Some(&json!("tool")))
        })
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
}

#[test]
fn a_tool_result_carries_the_delta_to_the_wire() -> Result<(), Box<dyn std::error::Error>> {
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::{CallId, Message};

    // Com delta: o modelo recebe o payload em TOON (§18/G6), não o `ToolOutcome`.
    let mut with_delta = request("m");
    with_delta.messages = vec![Message::ToolResult {
        call: CallId::new("c1"),
        outcome: ToolOutcome::Ok,
        delta: Some("r\nread.summary\nk\npath nota.txt".to_string()),
        tool_name: None,
    }];
    let body = super::encode_request(&with_delta, &super::EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let content = tool_content(&value);
    assert_eq!(content, Some("r\nread.summary\nk\npath nota.txt"));

    // Sem delta (recusa/erro): o efeito serializado, como antes.
    let mut no_delta = request("m");
    no_delta.messages = vec![Message::ToolResult {
        call: CallId::new("c1"),
        outcome: ToolOutcome::Timeout,
        delta: None,
        tool_name: None,
    }];
    let body = super::encode_request(&no_delta, &super::EncodeOptions::default())?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(tool_content(&value), Some("\"timeout\""));
    Ok(())
}

#[test]
fn delta_reasoning_accepts_all_field_variants() -> Result<(), Box<dyn std::error::Error>> {
    for case in [
        r#"{"reasoning_content":"a"}"#,
        r#"{"reasoning":"b"}"#,
        r#"{"reasoning_text":"c"}"#,
    ] {
        let delta: super::chunk::Delta = serde_json::from_str(case)?;
        assert!(delta.reasoning().is_some());
    }
    let empty: super::chunk::Delta = serde_json::from_str("{}")?;
    assert!(empty.reasoning().is_none());
    Ok(())
}

#[test]
fn tool_names_match_the_registry() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    use katu_policy::{ResolvedArgv, ResolvedPath, SearchMode, ToolArgs, ToolName, ToolUse};

    let cwd = ResolvedPath::from_canonical(PathBuf::from("/work"))?;
    let exec = ToolUse {
        name: ToolName::Exec,
        args: ToolArgs::Exec {
            argv: ResolvedArgv::new(vec!["ls".to_string()])?,
            cwd: cwd.clone(),
        },
        resolved_paths: Vec::new(),
        argv: None,
        cwd: cwd.clone(),
    };
    assert_eq!(super::encode::model_tool_name(&exec), "bash");
    assert_eq!(
        super::encode::tool_arguments(&exec).get("argv"),
        Some(&json!(["ls"]))
    );

    let find = ToolUse {
        name: ToolName::Search,
        args: ToolArgs::Search {
            root: cwd,
            query: "x".to_string(),
            mode: SearchMode::Find,
        },
        ..exec
    };
    assert_eq!(super::encode::model_tool_name(&find), "find");
    Ok(())
}
