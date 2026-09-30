//! Testes da codificação do dialeto `chat/completions`.

use katu_core::provider::{ModelSpec, ProviderRequest};
use serde_json::json;

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
    let body = super::encode_request(&request("m"))?;
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
