//! Codificação do pedido no dialeto **Responses API**.
//!
//! Ver a forma usada pelo `pi` (`api/openai-responses.ts`): `instructions` para o sistema,
//! `input` para o histórico, `max_output_tokens` (mínimo 16), `store: false` e `reasoning.effort`.

use katu_core::kernel::Message;
use katu_core::provider::{ProviderError, ProviderRequest, ToolDef};
use serde_json::{Map, Value, json};

use crate::openai::{EncodeOptions, model_tool_name, thinking_effort, tool_arguments};

/// Mínimo exigido pelo `OpenAI` Responses para `max_output_tokens`.
const MIN_OUTPUT_TOKENS: u32 = 16;

/// Serializa o pedido completo (JSON) para `POST /responses`.
///
/// # Errors
/// [`ProviderError::Decode`] se a serialização falhar (nunca em prática).
pub(crate) fn encode_request(
    request: &ProviderRequest,
    options: &EncodeOptions,
) -> Result<String, ProviderError> {
    let mut input = Vec::new();
    for message in &request.messages {
        if let Some(encoded) = encode_message(message)? {
            input.push(encoded);
        }
    }

    let mut body = Map::new();
    body.insert("model".to_string(), json!(request.model.model));
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        body.insert("instructions".to_string(), json!(system));
    }
    body.insert("input".to_string(), Value::Array(input));
    body.insert("stream".to_string(), json!(true));
    body.insert("store".to_string(), json!(false));
    if !request.tools.is_empty() {
        let tools = request.tools.iter().map(encode_tool).collect();
        body.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(max_tokens) = request.max_tokens.or(options.default_max_tokens) {
        body.insert(
            "max_output_tokens".to_string(),
            json!(max_tokens.max(MIN_OUTPUT_TOKENS)),
        );
    }
    if let Some(temperature) = request.temperature.or(options.default_temperature) {
        body.insert("temperature".to_string(), json!(f64::from(temperature)));
    }
    if let Some(key) = options.prompt_cache_key.as_deref() {
        body.insert("prompt_cache_key".to_string(), json!(key));
    }
    if let Some(retention) = options.prompt_cache_retention.as_deref() {
        body.insert("prompt_cache_retention".to_string(), json!(retention));
    }
    if let Some(effort) = thinking_effort(request.model.thinking) {
        body.insert("reasoning".to_string(), json!({"effort": effort}));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Codifica uma tool no formato da Responses API (plana).
fn encode_tool(tool: &ToolDef) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
    })
}

/// Codifica uma mensagem do histórico (ou ignora se desconhecida).
fn encode_message(message: &Message) -> Result<Option<Value>, ProviderError> {
    let encoded = match message {
        Message::User { text } => json!({"role": "user", "content": text}),
        Message::Assistant { text } => json!({"role": "assistant", "content": text}),
        Message::ToolCall { call, tool } => json!({
            "type": "function_call",
            "call_id": call.as_str(),
            "name": model_tool_name(tool),
            "arguments": tool_arguments(tool).to_string(),
        }),
        Message::ToolResult { call, outcome } => json!({
            "type": "function_call_output",
            "call_id": call.as_str(),
            "output": serde_json::to_string(outcome)
                .map_err(|error| ProviderError::Decode(error.to_string()))?,
        }),
        _ => return Ok(None),
    };
    Ok(Some(encoded))
}
