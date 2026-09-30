//! TOON (DF12/E06-T12) — modelo de valores e **formato colunar D39** ao modelo (ADR 0005).
//!
//! [`Value`] é a árvore usada pelas tools, pelo diagnóstico e pelo JSON. O **formato ao modelo**
//! é colunar ([`Table`]/[`project`]): um *stream* de tabelas com header autodescritivo
//! (`\x1e`/`\x1f`), projetado de [`Value`] ao emitir. Determinístico byte-a-byte.
//!
//! ```
//! use katu_core::toon::{Cell, RowTable, Section, emit};
//! let mut table = RowTable::new("symbols");
//! table.push(vec![Cell::text("s_1"), Cell::text("main")]);
//! assert_eq!(emit(&[Section::Rows(table)]), "\u{1e}symbols\ns_1\u{1f}main\n");
//! ```

mod aliases;
mod colunar;
mod project;

pub mod schema;

pub use aliases::Aliases;
pub use colunar::{Cell, RowTable, Section, emit};
pub use project::project;

/// Valor TOON: mapa ordenado, lista ou escalar.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Mapa ordenado (a ordem de inserção é contrato).
    Map(Vec<(String, Self)>),
    /// Lista.
    List(Vec<Self>),
    /// Texto.
    Str(String),
    /// Inteiro.
    Int(i64),
    /// Vírgula flutuante.
    Float(f64),
    /// Booleano.
    Bool(bool),
    /// Mapa **inline** (`{k: v, …}`), mesmo aninhado — para `page`/`cost` compactos.
    Flow(Vec<(String, Self)>),
    /// Texto multilinha emitido como **bloco literal** (sem escapes) — para conteúdo de código.
    /// Em *flow* cai para string citada; JSON serializa como string.
    Block(String),
}

impl Value {
    /// Constrói um mapa a partir de pares (a ordem canónica é preservada).
    #[must_use]
    pub const fn map(entries: Vec<(String, Self)>) -> Self {
        Self::Map(entries)
    }

    /// Constrói uma lista.
    #[must_use]
    pub const fn list(items: Vec<Self>) -> Self {
        Self::List(items)
    }

    /// Constrói um texto.
    #[must_use]
    pub fn str(text: impl Into<String>) -> Self {
        Self::Str(text.into())
    }

    /// Constrói um inteiro.
    #[must_use]
    pub const fn int(value: i64) -> Self {
        Self::Int(value)
    }

    /// Constrói um booleano.
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "construtor de escalar booleano: a API é `Value::bool(bool)`"
    )]
    #[must_use]
    pub const fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    /// Constrói um bloco literal (conteúdo de código, sem escapes).
    #[must_use]
    pub fn block(text: impl Into<String>) -> Self {
        Self::Block(text.into())
    }

    /// Constrói um mapa **inline** (renderizado em *flow*).
    #[must_use]
    pub const fn flow(entries: Vec<(String, Self)>) -> Self {
        Self::Flow(entries)
    }
}

use serde::ser::{Serialize, SerializeMap, Serializer};

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Map(entries) | Self::Flow(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
            Self::List(items) => serializer.collect_seq(items),
            Self::Str(text) | Self::Block(text) => serializer.serialize_str(text),
            Self::Int(number) => serializer.serialize_i64(*number),
            Self::Float(number) => serializer.serialize_f64(*number),
            Self::Bool(flag) => serializer.serialize_bool(*flag),
        }
    }
}

#[cfg(test)]
mod tests;
