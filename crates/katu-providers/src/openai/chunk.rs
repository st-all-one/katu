//! Estruturas wire do `chat.completion.chunk` (serde).

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct Chunk {
    #[serde(default)]
    pub(crate) choices: Vec<Choice>,
    pub(crate) usage: Option<UsageJson>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    pub(crate) delta: Option<Delta>,
    pub(crate) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Delta {
    pub(crate) content: Option<Value>,
    pub(crate) reasoning_content: Option<String>,
    pub(crate) tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolCallDelta {
    #[serde(default)]
    pub(crate) index: u32,
    pub(crate) id: Option<String>,
    pub(crate) function: Option<FunctionDelta>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FunctionDelta {
    pub(crate) name: Option<String>,
    pub(crate) arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageJson {
    pub(crate) prompt_tokens: Option<u64>,
    pub(crate) completion_tokens: Option<u64>,
    pub(crate) prompt_tokens_details: Option<PromptDetails>,
    pub(crate) completion_tokens_details: Option<CompletionDetails>,
    /// Tokens de entrada servidos por cache — dialetos que usam este nome (`DeepSeek`,
    /// `Qihoo` e alguns gateways `OpenAI`-compatible) no lugar de `prompt_tokens_details`.
    pub(crate) prompt_cache_hit_tokens: Option<u64>,
    /// Alguns gateways expõem o cache no topo do `usage`.
    pub(crate) cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PromptDetails {
    pub(crate) cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompletionDetails {
    pub(crate) reasoning_tokens: Option<u64>,
}

/// Extrai texto de um campo `content` (string ou lista de partes de texto).
pub(crate) fn text_of(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                if let Some(chunk) = part.get("text").and_then(Value::as_str) {
                    text.push_str(chunk);
                }
            }
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}
