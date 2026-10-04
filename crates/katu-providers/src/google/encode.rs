//! Codificação do pedido no dialeto Google Gemini (`generateContent`).
//!
//! O histórico vira `contents` (`user`/`model`); a instrução de sistema vira `systemInstruction`;
//! as tools viram `functionDeclarations`. Uma `functionResponse` precisa do **nome** da função,
//! que não vive no `ToolResult`: recupera-se do `ToolCall` anterior com o mesmo `CallId`.

use std::collections::BTreeMap;

use katu_core::diag::{Level, events};
use katu_core::kernel::Message;
use katu_core::provider::{ProviderError, ProviderRequest, Thinking, ToolDef};
use katu_core::report::tool_content;
use serde_json::{Map, Value, json};

use crate::openai::{EncodeOptions, model_tool_name, tool_arguments};

/// Orçamento de pensamento em tokens por grau (`None` = omitir, deixa o default do modelo).
fn thinking_budget(thinking: Thinking) -> Option<u32> {
    let _span = katu_core::trace_fn!("google::encode::thinking_budget");

    match thinking {
        Thinking::Low => Some(1024),
        Thinking::Medium => Some(8192),
        Thinking::High => Some(24_576),
        _ => None,
    }
}

/// Serializa o pedido completo (JSON) para `POST /models/<id>:streamGenerateContent`.
///
/// # Errors
/// [`ProviderError::Decode`] se a serialização falhar (nunca em prática).
pub(crate) fn encode_request(
    request: &ProviderRequest,
    options: &EncodeOptions,
) -> Result<String, ProviderError> {
    let _span = katu_core::fn_span!(
        Level::Debug,
        events::PROVIDER_REQUEST,
        "google::encode_request"
    );
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut contents = Vec::new();
    for message in &request.messages {
        if let Some(encoded) = encode_message(message, &mut names) {
            contents.push(encoded);
        }
    }

    let mut body = Map::new();
    body.insert("contents".to_string(), Value::Array(contents));
    if let Some(system) = request.system.as_deref().filter(|text| !text.is_empty()) {
        body.insert(
            "systemInstruction".to_string(),
            json!({"parts": [{"text": system}]}),
        );
    }
    if !request.tools.is_empty() {
        let declarations: Vec<Value> = request.tools.iter().map(encode_tool).collect();
        body.insert(
            "tools".to_string(),
            json!([{"functionDeclarations": declarations}]),
        );
    }
    let generation = generation_config(request, options);
    if !generation.is_empty() {
        body.insert("generationConfig".to_string(), Value::Object(generation));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| ProviderError::Decode(error.to_string()))
}

/// `generationConfig` (teto de tokens, temperatura e pensamento).
fn generation_config(request: &ProviderRequest, options: &EncodeOptions) -> Map<String, Value> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::PROVIDER_REQUEST,
        "google::generation_config"
    );
    let mut generation = Map::new();
    if let Some(max) = request.max_tokens.or(options.default_max_tokens) {
        generation.insert("maxOutputTokens".to_string(), json!(max));
    }
    if let Some(temperature) = request.temperature.or(options.default_temperature) {
        generation.insert("temperature".to_string(), json!(f64::from(temperature)));
    }
    if let Some(budget) = thinking_budget(request.model.thinking) {
        generation.insert(
            "thinkingConfig".to_string(),
            json!({"thinkingBudget": budget, "includeThoughts": true}),
        );
    }
    generation
}

/// Codifica uma tool em `functionDeclaration`.
fn encode_tool(tool: &ToolDef) -> Value {
    let _span = katu_core::trace_fn!("google::encode::encode_tool");

    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
    })
}

/// Codifica uma mensagem do histórico (ou ignora se desconhecida).
fn encode_message(message: &Message, names: &mut BTreeMap<String, String>) -> Option<Value> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::PROVIDER_REQUEST,
        "google::encode_message"
    );
    let encoded = match message {
        Message::User { text, .. } => json!({"role": "user", "parts": [{"text": text}]}),
        Message::Assistant { text } => json!({"role": "model", "parts": [{"text": text}]}),
        Message::ToolCall { call, tool } => {
            let name = model_tool_name(tool);
            names.insert(call.as_str().to_string(), name.clone());
            json!({
                "role": "model",
                "parts": [{"functionCall": {"name": name, "args": tool_arguments(tool)}}],
            })
        }
        Message::ToolResult {
            call,
            outcome,
            delta,
            ..
        } => {
            let name = names
                .get(call.as_str())
                .cloned()
                .unwrap_or_else(|| call.as_str().to_string());
            let result = tool_content(outcome, delta.as_deref());
            json!({
                "role": "user",
                "parts": [{"functionResponse": {"name": name, "response": {"result": result}}}],
            })
        }
        _ => return None,
    };
    Some(encoded)
}
