//! Estruturas wire do dialeto `messages` (Anthropic; serde).

use serde::Deserialize;

/// Evento do stream (`message_start`, `content_block_delta`, …).
#[derive(Debug, Deserialize)]
pub(crate) struct Event {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) index: Option<u32>,
    #[serde(default)]
    pub(crate) content_block: Option<ContentBlock>,
    #[serde(default)]
    pub(crate) delta: Option<Delta>,
    #[serde(default)]
    pub(crate) message: Option<MessageStart>,
    #[serde(default)]
    pub(crate) usage: Option<UsageJson>,
}

/// Bloco de conteúdo (`text`, `thinking`, `tool_use`).
#[derive(Debug, Deserialize)]
pub(crate) struct ContentBlock {
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
}

/// Delta de um bloco ou do `message_delta`.
#[derive(Debug, Deserialize)]
pub(crate) struct Delta {
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) thinking: Option<String>,
    #[serde(default)]
    pub(crate) partial_json: Option<String>,
    #[serde(default)]
    pub(crate) stop_reason: Option<String>,
}

/// Início de mensagem (`message_start`).
#[derive(Debug, Deserialize)]
pub(crate) struct MessageStart {
    #[serde(default)]
    pub(crate) usage: Option<UsageJson>,
}

/// Contabilização de tokens.
#[allow(
    clippy::struct_field_names,
    reason = "nomes do wire Anthropic terminam em `_tokens`"
)]
#[derive(Debug, Deserialize, Default)]
pub(crate) struct UsageJson {
    #[serde(default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) output_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cache_read_input_tokens: Option<u64>,
}
