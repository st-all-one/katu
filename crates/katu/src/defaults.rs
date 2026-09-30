//! Padrões do projeto (E20-T17): a config efetiva alimenta o default dos comandos.
//!
//! O projeto **vence** o global (merge chave a chave). Um valor ausente fica `None` — os comandos
//! aplicam então o seu próprio default; nada se inventa.

use std::path::Path;

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
    /// Compactação automática do histórico.
    pub(crate) auto_compact: Option<bool>,
    /// Limite de recall por omissão.
    pub(crate) recall_limit: Option<usize>,
}

/// Padrões do projeto atual (raiz descoberta a subir do diretório atual).
pub(crate) fn current() -> Defaults {
    let fs = StdFs;
    let start = std::env::current_dir().unwrap_or_default();
    let root = discover_root(&fs, &start);
    from_root(&root)
}

/// Padrões de um projeto.
fn from_root(root: &Path) -> Defaults {
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
        auto_compact: boolean(&table, "behavior.auto_compact"),
        recall_limit: integer(&table, "recall.default_limit")
            .and_then(|value| usize::try_from(value).ok()),
    }
}

/// Texto **não vazio** de uma chave.
fn text(table: &toml::Table, key: &str) -> Option<String> {
    match config::get_key(table, key) {
        Some(toml::Value::String(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Booleano de uma chave.
fn boolean(table: &toml::Table, key: &str) -> Option<bool> {
    match config::get_key(table, key) {
        Some(toml::Value::Boolean(value)) => Some(value),
        _ => None,
    }
}

/// Inteiro de uma chave.
fn integer(table: &toml::Table, key: &str) -> Option<i64> {
    match config::get_key(table, key) {
        Some(toml::Value::Integer(value)) => Some(value),
        _ => None,
    }
}
