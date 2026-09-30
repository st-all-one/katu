//! Codificação do pedido no dialeto `chat/completions`.

use katu_core::kernel::Message;
use katu_core::provider::{ProviderError, ProviderRequest, Thinking, ToolDef};
use katu_policy::{SearchMode, ToolArgs, ToolName, ToolUse};
use serde_json::{Map, Value, json};

/// Serializa o pedido completo (JSON) para o endpoint.
///
/// # Errors
/// [`ProviderError::Decode`] se a serialização falhar (nunca em prática).
pub(crate) fn encode_request(request: &ProviderRequest) -> Result<String, ProviderError> {
    let mut messages = Vec::new();
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        messages.push(json!({"role": "system", "content": system}));
    }
    for message in &request.messages {
        if let Some(encoded) = encode_message(message)? {
            messages.push(encoded);
        }
    }

    let mut body = Map::new();
    body.insert("model".to_string(), json!(request.model.model));
    body.insert("messages".to_string(), Value::Array(messages));
    body.insert("stream".to_string(), json!(true));
    body.insert("stream_options".to_string(), json!({"include_usage": true}));
    if !request.tools.is_empty() {
        let tools = request.tools.iter().map(encode_tool).collect();
        body.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".to_string(), json!(max_tokens));
    }
    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_string(), json!(f64::from(temperature)));
    }
    if let Some(effort) = thinking_effort(request.model.thinking) {
        body.insert("reasoning_effort".to_string(), json!(effort));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Codifica uma tool no formato `OpenAI`.
fn encode_tool(tool: &ToolDef) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

/// Codifica uma mensagem do histórico (ou ignora se desconhecida).
fn encode_message(message: &Message) -> Result<Option<Value>, ProviderError> {
    let encoded = match message {
        Message::User { text } => json!({"role": "user", "content": text}),
        Message::Assistant { text } => json!({"role": "assistant", "content": text}),
        Message::ToolCall { call, tool } => json!({
            "role": "assistant",
            "content": Value::Null,
            "tool_calls": [{
                "id": call.as_str(),
                "type": "function",
                "function": {
                    "name": model_tool_name(tool),
                    "arguments": tool_arguments(tool).to_string(),
                }
            }]
        }),
        Message::ToolResult { call, outcome } => json!({
            "role": "tool",
            "tool_call_id": call.as_str(),
            "content": serde_json::to_string(outcome)
                .map_err(|error| ProviderError::Decode(error.to_string()))?,
        }),
        _ => return Ok(None),
    };
    Ok(Some(encoded))
}

/// Nome ao modelo de um uso de tool (registry: `exec`→`bash`, `search`→`grep|find|ls`).
#[must_use]
pub(crate) fn model_tool_name(tool: &ToolUse) -> String {
    match tool.name {
        ToolName::Exec => "bash".to_string(),
        ToolName::Search => match &tool.args {
            ToolArgs::Search {
                mode: SearchMode::Find,
                ..
            } => "find".to_string(),
            ToolArgs::Search {
                mode: SearchMode::Ls,
                ..
            } => "ls".to_string(),
            _ => "grep".to_string(),
        },
        ToolName::MemoryWrite | ToolName::MemoryRecall => "memory".to_string(),
        other => other.as_str().to_string(),
    }
}

/// Reconstrói os argumentos ao modelo a partir do uso tipado (melhor esforço).
///
/// Nota: o log guarda o [`ToolUse`] **resolvido**, não os argumentos crus do modelo; a
/// reconstrução é fiel para as chaves que o tipo preserva (E04/E12 — dívida registada).
#[must_use]
pub(crate) fn tool_arguments(tool: &ToolUse) -> Value {
    match &tool.args {
        ToolArgs::Read { path } | ToolArgs::Edit { path } | ToolArgs::Trash { path } => {
            json!({"path": path})
        }
        ToolArgs::Write { path, bytes } => json!({"path": path, "bytes": bytes}),
        ToolArgs::Move { from, to } => json!({"from": from, "to": to}),
        ToolArgs::Exec { argv, cwd } => json!({"argv": argv, "cwd": cwd}),
        ToolArgs::Search { root, query, .. } => json!({"query": query, "root": root}),
        _ => Value::Object(Map::new()),
    }
}

/// Mapeia o grau de pensamento para `reasoning_effort` (dialeto `OpenAI`).
fn thinking_effort(thinking: Thinking) -> Option<&'static str> {
    match thinking {
        Thinking::Low => Some("low"),
        Thinking::Medium => Some("medium"),
        Thinking::High => Some("high"),
        _ => None,
    }
}
