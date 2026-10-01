//! Codificação do pedido no dialeto `chat/completions`.

use katu_core::diag::{Level, events};
use katu_core::kernel::Message;
use katu_core::provider::{ProviderError, ProviderRequest, Thinking, ToolDef};
use katu_core::report::tool_content;
use katu_policy::{SearchMode, ToolArgs, ToolName, ToolUse};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::catalog::MaxTokensField;

/// Parametrização do wire que não vem do pedido (modelo/catálogo/config).
#[derive(Debug, Clone, Default)]
pub(crate) struct EncodeOptions {
    /// Campo do teto de tokens de saída.
    pub max_tokens_field: MaxTokensField,
    /// `prompt_cache_key` (cache de prefixo; famílias `OpenAI`).
    pub prompt_cache_key: Option<String>,
    /// `prompt_cache_retention` (ex.: `"24h"`).
    pub prompt_cache_retention: Option<String>,
    /// `reasoning_format` (`llama.cpp`: `"parsed"`).
    pub reasoning_format: Option<String>,
    /// Teto de tokens por omissão do provider (usado se o pedido não trouxer).
    pub default_max_tokens: Option<u32>,
    /// Temperatura por omissão do provider (usada se o pedido não trouxer).
    pub default_temperature: Option<f32>,
}

/// Corpo do pedido `chat/completions` (serialização direta, sem árvore `Value` intermédia).
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<MessageJson<'a>>,
    stream: bool,
    stream_options: StreamOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolJson<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_cache_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_cache_retention: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

/// `stream_options` do pedido.
#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// Uma tool no formato `OpenAI`.
#[derive(Serialize)]
struct ToolJson<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: FunctionJson<'a>,
}

/// A função de uma tool.
#[derive(Serialize)]
struct FunctionJson<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

/// Mensagem do histórico (uma forma por variante).
#[derive(Serialize)]
#[serde(untagged)]
enum MessageJson<'a> {
    Text(TextMessage<'a>),
    ToolCall(ToolCallMessage<'a>),
    ToolResult(ToolResultMessage<'a>),
}

/// Mensagem de texto (`system`/`user`/`assistant`).
#[derive(Serialize)]
struct TextMessage<'a> {
    role: &'static str,
    content: &'a str,
}

/// Mensagem de pedido de tool (`assistant` + `tool_calls`).
#[derive(Serialize)]
struct ToolCallMessage<'a> {
    role: &'static str,
    content: Option<()>,
    tool_calls: Vec<ToolCallJson<'a>>,
}

/// Mensagem de resultado de tool.
#[derive(Serialize)]
struct ToolResultMessage<'a> {
    role: &'static str,
    tool_call_id: &'a str,
    content: String,
}

/// Uma tool call do histórico.
#[derive(Serialize)]
struct ToolCallJson<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ToolCallFunction,
}

/// Nome + argumentos de uma tool call.
#[derive(Serialize)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

/// Serializa o pedido completo (JSON) para o endpoint.
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
        "openai::encode_request"
    );
    let mut messages = Vec::with_capacity(request.messages.len().saturating_add(1));
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        messages.push(MessageJson::Text(TextMessage {
            role: "system",
            content: system,
        }));
    }
    for message in &request.messages {
        if let Some(encoded) = encode_message(message) {
            messages.push(encoded);
        }
    }

    let max_tokens = request.max_tokens.or(options.default_max_tokens);
    let mut body = ChatRequest {
        model: &request.model.model,
        messages,
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
        tools: (!request.tools.is_empty()).then(|| request.tools.iter().map(encode_tool).collect()),
        max_tokens: None,
        max_completion_tokens: None,
        temperature: request
            .temperature
            .or(options.default_temperature)
            .map(f64::from),
        prompt_cache_key: options.prompt_cache_key.as_deref(),
        prompt_cache_retention: options.prompt_cache_retention.as_deref(),
        reasoning_format: options.reasoning_format.as_deref(),
        reasoning_effort: thinking_effort(request.model.thinking),
    };
    match options.max_tokens_field {
        MaxTokensField::MaxTokens => body.max_tokens = max_tokens,
        MaxTokensField::MaxCompletionTokens => body.max_completion_tokens = max_tokens,
    }
    serde_json::to_string(&body).map_err(|error| ProviderError::Decode(error.to_string()))
}

/// Codifica uma tool no formato `OpenAI`.
fn encode_tool(tool: &ToolDef) -> ToolJson<'_> {
    let _span = katu_core::trace_fn!("openai::encode::encode_tool");

    ToolJson {
        kind: "function",
        function: FunctionJson {
            name: tool.name.as_str(),
            description: tool.description.as_str(),
            parameters: &tool.parameters,
        },
    }
}

/// Codifica uma mensagem do histórico (ou ignora se desconhecida).
fn encode_message(message: &Message) -> Option<MessageJson<'_>> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::PROVIDER_REQUEST,
        "openai::encode_message"
    );
    let encoded = match message {
        Message::User { text } => MessageJson::Text(TextMessage {
            role: "user",
            content: text,
        }),
        Message::Assistant { text } => MessageJson::Text(TextMessage {
            role: "assistant",
            content: text,
        }),
        Message::ToolCall { call, tool } => MessageJson::ToolCall(ToolCallMessage {
            role: "assistant",
            content: Some(()),
            tool_calls: vec![ToolCallJson {
                id: call.as_str(),
                kind: "function",
                function: ToolCallFunction {
                    name: model_tool_name(tool),
                    arguments: tool_arguments(tool).to_string(),
                },
            }],
        }),
        Message::ToolResult {
            call,
            outcome,
            delta,
        } => MessageJson::ToolResult(ToolResultMessage {
            role: "tool",
            tool_call_id: call.as_str(),
            content: tool_content(outcome, delta.as_deref()),
        }),
        _ => return None,
    };
    Some(encoded)
}

/// Nome ao modelo de um uso de tool (registry: `exec`→`bash`, `search`→`grep|find|ls`).
#[must_use]
pub(crate) fn model_tool_name(tool: &ToolUse) -> String {
    let _span = katu_core::trace_fn!("openai::encode::model_tool_name");

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
    let _span = katu_core::trace_fn!("openai::encode::tool_arguments");

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
pub(crate) fn thinking_effort(thinking: Thinking) -> Option<&'static str> {
    let _span = katu_core::trace_fn!("openai::encode::thinking_effort");

    match thinking {
        Thinking::Low => Some("low"),
        Thinking::Medium => Some("medium"),
        Thinking::High => Some("high"),
        _ => None,
    }
}
