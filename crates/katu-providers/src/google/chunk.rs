//! Estruturas wire do dialeto Google Gemini (`streamGenerateContent`; serde).
//!
//! Cada evento SSE é um `Chunk` com candidatos; as `parts` trazem texto, raciocínio
//! (`thought: true`) ou `functionCall` (completo). A contabilização vem em `usageMetadata`.

use serde::Deserialize;

/// Um evento do stream.
#[derive(Debug, Deserialize)]
pub(crate) struct Chunk {
    #[serde(default)]
    pub(crate) candidates: Vec<Candidate>,
    #[serde(default, rename = "usageMetadata")]
    pub(crate) usage_metadata: Option<UsageJson>,
}

/// Um candidato (resposta possível).
#[derive(Debug, Deserialize)]
pub(crate) struct Candidate {
    #[serde(default)]
    pub(crate) content: Option<Content>,
    #[serde(default, rename = "finishReason")]
    pub(crate) finish_reason: Option<String>,
}

/// Conteúdo do candidato.
#[derive(Debug, Deserialize)]
pub(crate) struct Content {
    #[serde(default)]
    pub(crate) parts: Vec<Part>,
}

/// Uma parte (`text`, raciocínio ou chamada de função).
#[derive(Debug, Deserialize)]
pub(crate) struct Part {
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) thought: bool,
    #[serde(default, rename = "functionCall")]
    pub(crate) function_call: Option<FunctionCall>,
}

/// Chamada de função pedida pelo modelo.
#[derive(Debug, Deserialize)]
pub(crate) struct FunctionCall {
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) args: serde_json::Value,
}

/// Contabilização de tokens (base `provider_reported`).
#[allow(
    clippy::struct_field_names,
    reason = "nomes do wire Google terminam em `_token_count`"
)]
#[derive(Debug, Deserialize, Default)]
pub(crate) struct UsageJson {
    #[serde(default, rename = "promptTokenCount")]
    pub(crate) prompt_token_count: Option<u64>,
    #[serde(default, rename = "candidatesTokenCount")]
    pub(crate) candidates_token_count: Option<u64>,
    #[serde(default, rename = "cachedContentTokenCount")]
    pub(crate) cached_content_token_count: Option<u64>,
}
