//! Catálogo de modelos → dialeto (E12-T06/T10).
//!
//! O gateway do built-in expõe **quatro dialetos**; qual usar depende do id do modelo. O catálogo
//! é a tabela `model → {dialect, context_limit, max_tokens_field, prompt_cache, reasoning}` que
//! encaminha o pedido e parametriza o wire. Modelos desconhecidos caem no dialeto por omissão do
//! adaptador (o `pi` resolve isto por catálogo gerado; aqui é declarativo, ver [`crate::declarative`]).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Dialeto de wire normalizado pelo trait `Provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum Dialect {
    /// `chat/completions` (`OpenAI`-compatible).
    #[serde(rename = "chat/completions")]
    ChatCompletions,
    /// `responses` (`OpenAI` Responses API).
    #[serde(rename = "responses")]
    Responses,
    /// `messages` (Anthropic Messages).
    #[serde(rename = "messages")]
    Messages,
    /// `models/<id>` (Google Gemini).
    #[serde(rename = "google")]
    Google,
}

impl Dialect {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat/completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::Google => "google",
        }
    }
}

/// Nome do campo do teto de tokens de saída no dialeto `OpenAI`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub enum MaxTokensField {
    /// `max_tokens` (dialetos antigos, `llama.cpp`, a maioria dos gateways).
    #[serde(rename = "max_tokens")]
    #[default]
    MaxTokens,
    /// `max_completion_tokens` (modelos `OpenAI` recentes).
    #[serde(rename = "max_completion_tokens")]
    MaxCompletionTokens,
}

impl MaxTokensField {
    /// Nome do campo no JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaxTokens => "max_tokens",
            Self::MaxCompletionTokens => "max_completion_tokens",
        }
    }
}

/// Metadados de um modelo: roteamento e parametrização do wire.
#[allow(
    clippy::struct_excessive_bools,
    reason = "dois flags independentes de capacidade (cache e raciocínio); não há estado inválido"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    /// Identificador do modelo no endpoint.
    pub id: String,
    /// Dialeto a usar.
    pub dialect: Dialect,
    /// Janela de contexto, quando conhecida.
    pub context_limit: Option<u32>,
    /// Campo do teto de tokens de saída.
    pub max_tokens_field: MaxTokensField,
    /// Suporta `prompt_cache_key`/`prompt_cache_retention` (famílias `OpenAI`).
    pub prompt_cache: bool,
    /// Emite raciocínio (thinking) e aceita grau de pensamento.
    pub reasoning: bool,
    /// `reasoning_format` específico do gateway (ex.: `"parsed"`).
    pub reasoning_format: Option<String>,
}

impl ModelEntry {
    /// Entrada mínima (dialeto explícito).
    #[must_use]
    pub fn new(id: impl Into<String>, dialect: Dialect) -> Self {
        Self {
            id: id.into(),
            dialect,
            context_limit: None,
            max_tokens_field: MaxTokensField::MaxTokens,
            prompt_cache: false,
            reasoning: false,
            reasoning_format: None,
        }
    }

    /// Fixa a janela de contexto.
    #[must_use]
    pub const fn with_context_limit(mut self, limit: u32) -> Self {
        self.context_limit = Some(limit);
        self
    }

    /// Marca suporte a cache de prefixo.
    #[must_use]
    pub const fn with_prompt_cache(mut self) -> Self {
        self.prompt_cache = true;
        self
    }

    /// Marca suporte a raciocínio.
    #[must_use]
    pub const fn with_reasoning(mut self) -> Self {
        self.reasoning = true;
        self
    }

    /// Fixa o campo do teto de tokens.
    #[must_use]
    pub const fn with_max_tokens_field(mut self, field: MaxTokensField) -> Self {
        self.max_tokens_field = field;
        self
    }

    /// Fixa um `reasoning_format` para o gateway.
    #[must_use]
    pub fn with_reasoning_format(mut self, format: impl Into<String>) -> Self {
        self.reasoning_format = Some(format.into());
        self
    }
}

/// Mapa de `model` para `ModelEntry`.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: BTreeMap<String, ModelEntry>,
}

impl Catalog {
    /// Catálogo vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insere/substitui uma entrada.
    pub fn insert(&mut self, entry: ModelEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    /// Procura um modelo.
    #[must_use]
    pub fn lookup(&self, model: &str) -> Option<&ModelEntry> {
        self.entries.get(model)
    }

    /// Número de entradas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` se vazio.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Catalog, Dialect, MaxTokensField, ModelEntry};

    #[test]
    fn lookup_and_insert_replace_by_id() {
        let mut catalog = Catalog::new();
        assert!(catalog.is_empty());
        catalog.insert(ModelEntry::new("m", Dialect::ChatCompletions));
        catalog.insert(ModelEntry::new("m", Dialect::Responses));
        assert_eq!(catalog.len(), 1);
        assert_eq!(
            catalog.lookup("m").map(|entry| entry.dialect),
            Some(Dialect::Responses)
        );
        assert!(catalog.lookup("other").is_none());
    }

    #[test]
    fn builders_set_the_wire_parameters() {
        let entry = ModelEntry::new("gpt", Dialect::Responses)
            .with_context_limit(400_000)
            .with_prompt_cache()
            .with_reasoning()
            .with_max_tokens_field(MaxTokensField::MaxCompletionTokens);
        assert_eq!(entry.context_limit, Some(400_000));
        assert!(entry.prompt_cache);
        assert!(entry.reasoning);
        assert_eq!(entry.max_tokens_field.as_str(), "max_completion_tokens");
    }
}
