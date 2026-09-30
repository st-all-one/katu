//! Estruturas wire do dialeto `responses` (serde).
//!
//! O stream identifica o evento pelo campo `type` dentro do JSON `data:` (não depende da linha
//! `event:`), o que simplifica o parser SSE.

use serde::Deserialize;

/// Evento do stream (`response.*`).
#[derive(Debug, Deserialize)]
pub(crate) struct Event {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) delta: Option<String>,
    #[serde(default)]
    pub(crate) item_id: Option<String>,
    #[serde(default)]
    pub(crate) item: Option<Item>,
    #[serde(default)]
    pub(crate) response: Option<Response>,
    #[serde(default)]
    pub(crate) usage: Option<UsageJson>,
}

/// Item (`message`, `function_call`, …).
#[derive(Debug, Deserialize)]
pub(crate) struct Item {
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) call_id: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) arguments: Option<String>,
}

/// Objeto `response` (em `response.completed`).
#[derive(Debug, Deserialize)]
pub(crate) struct Response {
    #[serde(default)]
    pub(crate) usage: Option<UsageJson>,
}

/// Contabilização de tokens.
#[derive(Debug, Deserialize, Default)]
pub(crate) struct UsageJson {
    #[serde(default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) output_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) input_tokens_details: Option<InputDetails>,
    #[serde(default)]
    pub(crate) output_tokens_details: Option<OutputDetails>,
}

/// Detalhe da entrada (cache).
#[derive(Debug, Deserialize)]
pub(crate) struct InputDetails {
    #[serde(default)]
    pub(crate) cached_tokens: Option<u64>,
}

/// Detalhe da saída (raciocínio).
#[derive(Debug, Deserialize)]
pub(crate) struct OutputDetails {
    #[serde(default)]
    pub(crate) reasoning_tokens: Option<u64>,
}
