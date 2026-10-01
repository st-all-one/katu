//! Padrões do projeto (E20-T17): a config efetiva alimenta o default dos comandos.
//!
//! O projeto **vence** o global (merge chave a chave). Um valor ausente fica `None` — os comandos
//! aplicam então o seu próprio default; nada se inventa.

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::kernel::discover_root;

use crate::config;
use crate::ports::StdFs;

/// Padrões extraídos da config efetiva (projeto > global).
#[derive(Debug, Default)]
pub(crate) struct Defaults {
    /// Provider por omissão.
    pub(crate) provider: Option<String>,
    /// Modelo por omissão.
    pub(crate) model: Option<String>,
    /// Base URL por omissão.
    pub(crate) base: Option<String>,
    /// Grau de pensamento por omissão (`off`/`low`/`medium`/`high`).
    pub(crate) thinking: Option<String>,
    /// Compactação automática do histórico.
    pub(crate) auto_compact: Option<bool>,
    /// Secção `estado` no prime (Q-04).
    pub(crate) prompt_state: Option<bool>,
    /// Política de seleção do contexto (Q-02b/Q-03): `suffix`/`utility`.
    pub(crate) context_selection: Option<String>,
    /// Durabilidade do log (ADR 0024/P-01): `event`/`turn`.
    pub(crate) durability: Option<String>,
    /// Saída estruturada do provider (B1/W8-1).
    pub(crate) structured_output: Option<bool>,
    /// Limite de recall por omissão.
    pub(crate) recall_limit: Option<usize>,
    /// Embeddings (E20-T17): a **segunda IA**, externa e plugável.
    pub(crate) embeddings: EmbeddingDefaults,
}

/// Configuração do serviço de embeddings (E20-T17): serviço externo por URL + modelo.
#[derive(Debug, Default, Clone)]
pub(crate) struct EmbeddingDefaults {
    /// URL base (`http://host:porta/v1`); ausente/vazio → embeddings `off`.
    pub(crate) url: Option<String>,
    /// Modelo de embeddings.
    pub(crate) model: Option<String>,
    /// Comando opcional para lançar o serviço (reservado; não lança sozinho).
    pub(crate) command: Option<String>,
}

/// Padrões do projeto atual (raiz descoberta a subir do diretório atual).
pub(crate) fn current() -> Defaults {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_LOAD, "defaults::current");
    let fs = StdFs;
    let start = std::env::current_dir().unwrap_or_default();
    let root = discover_root(&fs, &start);
    from_root(&root)
}

/// Padrões de um projeto.
pub(crate) fn from_root(root: &Path) -> Defaults {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_LOAD, "defaults::from_root");
    let mut table = match config::global_path() {
        Ok(path) => config::load(&path).unwrap_or_default(),
        Err(_) => toml::Table::new(),
    };
    if let Ok(project) = config::load(&config::project_path(root)) {
        config::merge(&mut table, &project);
    }
    Defaults {
        provider: text(&table, "provider"),
        model: text(&table, "model"),
        base: text(&table, "base"),
        thinking: text(&table, "thinking"),
        auto_compact: boolean(&table, "behavior.auto_compact"),
        prompt_state: boolean(&table, "behavior.prompt_state"),
        context_selection: text(&table, "behavior.context_selection"),
        durability: text(&table, "behavior.durability"),
        structured_output: boolean(&table, "provider.structured_output"),
        recall_limit: integer(&table, "recall.default_limit")
            .and_then(|value| usize::try_from(value).ok()),
        embeddings: EmbeddingDefaults {
            url: text(&table, "embeddings.url"),
            model: text(&table, "embeddings.model"),
            command: text(&table, "embeddings.command"),
        },
    }
}

/// Texto **não vazio** de uma chave.
fn text(table: &toml::Table, key: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("defaults::text");

    match config::get_key(table, key) {
        Some(toml::Value::String(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Booleano de uma chave.
fn boolean(table: &toml::Table, key: &str) -> Option<bool> {
    let _span = katu_core::trace_fn!("defaults::boolean");

    match config::get_key(table, key) {
        Some(toml::Value::Boolean(value)) => Some(value),
        _ => None,
    }
}

/// Inteiro de uma chave.
fn integer(table: &toml::Table, key: &str) -> Option<i64> {
    let _span = katu_core::trace_fn!("defaults::integer");

    match config::get_key(table, key) {
        Some(toml::Value::Integer(value)) => Some(value),
        _ => None,
    }
}
