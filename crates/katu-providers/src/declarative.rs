//! Providers declarativos (E12-T02): um JSON por provider, no estilo do `goose`.
//!
//! O built-in (`opencode go/zen`) tem adaptador próprio, afinado; os **demais** providers entram
//! por definição declarativa (`engine` + `base_url` + catálogo de modelos), o que dá dialetos
//! `OpenAI`/Anthropic/Google com custo marginal ~zero, reusando os mesmos adaptadores de wire. A
//! chave de API é lida pela borda e injetada ([`Declarative::with_api_key`]); nada aqui toca o
//! ambiente.

use serde::Deserialize;

use crate::catalog::{Catalog, Dialect, MaxTokensField, ModelEntry};

mod provider;
pub use provider::Declarative;

/// Motor de wire de um provider declarativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[non_exhaustive]
pub enum Engine {
    /// `chat/completions` (`OpenAI`-compatible).
    #[serde(rename = "openai")]
    #[default]
    OpenAi,
    /// `OpenAI` Responses API.
    #[serde(rename = "openai-responses")]
    OpenAiResponses,
    /// Anthropic Messages.
    #[serde(rename = "anthropic")]
    Anthropic,
    /// Google Gemini.
    #[serde(rename = "google")]
    Google,
}

impl Engine {
    /// Dialeto por omissão do motor.
    #[must_use]
    pub const fn dialect(self) -> Dialect {
        match self {
            Self::OpenAi => Dialect::ChatCompletions,
            Self::OpenAiResponses => Dialect::Responses,
            Self::Anthropic => Dialect::Messages,
            Self::Google => Dialect::Google,
        }
    }
}

/// Modelo de uma definição declarativa.
#[allow(
    clippy::struct_excessive_bools,
    reason = "DTO declarativo: flags opcionais de capacidade do modelo"
)]
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntrySpec {
    /// Identificador do modelo.
    pub name: String,
    /// Dialeto (por omissão, o do [`Engine`]).
    #[serde(default)]
    pub dialect: Option<Dialect>,
    /// Janela de contexto.
    #[serde(default)]
    pub context_limit: Option<u32>,
    /// Suporta cache de prefixo.
    #[serde(default)]
    pub prompt_cache: Option<bool>,
    /// Emite raciocínio.
    #[serde(default)]
    pub reasoning: Option<bool>,
    /// `reasoning_format` específico do gateway.
    #[serde(default)]
    pub reasoning_format: Option<String>,
}

/// Definição declarativa de um provider.
#[allow(
    clippy::struct_excessive_bools,
    reason = "DTO declarativo: defaults independentes de capacidade"
)]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProviderSpec {
    /// Identificador estável.
    pub name: String,
    /// Motor de wire.
    #[serde(default)]
    pub engine: Engine,
    /// Base `OpenAI`-compatible (sem barra final).
    pub base_url: String,
    /// Variável de ambiente da chave (lida pela borda, nunca aqui).
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Cabeçalho de afinidade de sessão (ex.: `x-opencode-session`).
    #[serde(default)]
    pub session_id_header: Option<String>,
    /// Campo do teto de tokens por omissão.
    #[serde(default)]
    pub default_max_tokens_field: MaxTokensField,
    /// Raciocínio por omissão.
    #[serde(default)]
    pub default_reasoning: bool,
    /// Cache de prefixo por omissão.
    #[serde(default)]
    pub default_prompt_cache: bool,
    /// Catálogo de modelos.
    #[serde(default)]
    pub models: Vec<ModelEntrySpec>,
}

/// Erro ao carregar uma definição declarativa.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpecError {
    /// JSON inválido.
    #[error("definição declarativa inválida: {0}")]
    Parse(String),
}

impl ProviderSpec {
    /// Carrega de um JSON.
    ///
    /// # Errors
    /// [`SpecError::Parse`] se o JSON não seguir o formato.
    pub fn from_json(json: &str) -> Result<Self, SpecError> {
        serde_json::from_str(json).map_err(|error| SpecError::Parse(error.to_string()))
    }

    /// Catálogo derivado da definição.
    #[must_use]
    pub fn catalog(&self) -> Catalog {
        let mut catalog = Catalog::new();
        for model in &self.models {
            let dialect = model.dialect.unwrap_or_else(|| self.engine.dialect());
            let mut entry = ModelEntry::new(model.name.as_str(), dialect)
                .with_max_tokens_field(self.default_max_tokens_field);
            entry.context_limit = model.context_limit;
            entry.prompt_cache = model.prompt_cache.unwrap_or(self.default_prompt_cache);
            entry.reasoning = model.reasoning.unwrap_or(self.default_reasoning);
            entry.reasoning_format.clone_from(&model.reasoning_format);
            catalog.insert(entry);
        }
        catalog
    }

    /// Definição curada do `opencode zen` (embutida).
    #[must_use]
    pub fn opencode_zen() -> Self {
        embedded(include_str!("../providers/opencode_zen.json"))
    }

    /// Definição curada do `opencode go` (embutida; exige afinidade de sessão).
    #[must_use]
    pub fn opencode_go() -> Self {
        embedded(include_str!("../providers/opencode_go.json"))
    }

    /// Definição da `OpenAI` (Responses API), exemplo do caminho declarativo.
    #[must_use]
    pub fn openai() -> Self {
        embedded(include_str!("../providers/openai.json"))
    }
}

/// Carrega uma definição embutida; um ficheiro malformado é apanhado pelo teste do módulo.
fn embedded(json: &str) -> ProviderSpec {
    ProviderSpec::from_json(json).unwrap_or_default()
}

#[cfg(test)]
mod tests;
