//! Codificação do pedido no dialeto `messages` (Anthropic).

use katu_core::kernel::Message;
use katu_core::provider::{ProviderError, ProviderRequest, ToolDef};
use serde_json::{Map, Value, json};

use crate::openai::{EncodeOptions, model_tool_name, tool_arguments};

/// `max_tokens` é obrigatório na Anthropic; usa-se este valor se o pedido não trouxer nenhum.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Serializa o pedido completo (JSON) para `POST /messages`.
///
/// # Errors
/// [`ProviderError::Decode`] se a serialização falhar (nunca em prática).
pub(crate) fn encode_request(
    request: &ProviderRequest,
    options: &EncodeOptions,
) -> Result<String, ProviderError> {
    let mut messages = Vec::new();
    for message in &request.messages {
        if let Some(encoded) = encode_message(message)? {
            messages.push(encoded);
        }
    }

    let mut body = Map::new();
    body.insert("model".to_string(), json!(request.model.model));
    body.insert("messages".to_string(), Value::Array(messages));
    body.insert(
        "max_tokens".to_string(),
        json!(
            request
                .max_tokens
                .or(options.default_max_tokens)
                .unwrap_or(DEFAULT_MAX_TOKENS)
        ),
    );
    body.insert("stream".to_string(), json!(true));
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        body.insert("system".to_string(), json!(system));
    }
    if !request.tools.is_empty() {
        let tools = request.tools.iter().map(encode_tool).collect();
        body.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(temperature) = request.temperature.or(options.default_temperature) {
        body.insert("temperature".to_string(), json!(f64::from(temperature)));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Codifica uma tool no formato Anthropic (`input_schema`).
fn encode_tool(tool: &ToolDef) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.parameters,
    })
}

/// Codifica uma mensagem do histórico (ou ignora se desconhecida).
fn encode_message(message: &Message) -> Result<Option<Value>, ProviderError> {
    let encoded = match message {
        Message::User { text } => {
            json!({"role": "user", "content": [{"type": "text", "text": text}]})
        }
        Message::Assistant { text } => {
            json!({"role": "assistant", "content": [{"type": "text", "text": text}]})
        }
        Message::ToolCall { call, tool } => json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": call.as_str(),
                "name": model_tool_name(tool),
                "input": tool_arguments(tool),
            }]
        }),
        Message::ToolResult { call, outcome } => json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": call.as_str(),
                "content": serde_json::to_string(outcome)
                    .map_err(|error| ProviderError::Decode(error.to_string()))?,
            }]
        }),
        _ => return Ok(None),
    };
    Ok(Some(encoded))
}
