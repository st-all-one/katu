//! TOON — subconjunto documentado para a **saída das tools ao modelo** (DF12/E06-T12).
//!
//! Contrato de bytes (adaptado da spec `TOON` do knudge): **canónico na emissão**, **sem `null`**
//! (campos opcionais são **omitidos**), vazios omitidos, **ordem canónica** (a ordem de inserção do
//! [`Value::Map`]). Determinístico byte-a-byte — é o que permite cache e prompt estável.
//!
//! ```
//! use katu_core::toon::{Value, emit};
//! let doc = Value::map(vec![
//!     ("kind".to_string(), Value::str("read.summary")),
//!     ("loc".to_string(), Value::int(142)),
//!     ("imports".to_string(), Value::list(vec![Value::str("jwt"), Value::str("db")])),
//! ]);
//! assert_eq!(emit(&doc), "kind: read.summary\nloc: 142\nimports: [jwt, db]\n");
//! ```

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
}

/// Emite um documento TOON canónico (termina em `\n`).
#[must_use]
pub fn emit(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::Map(entries) => emit_map(&mut out, entries, 0),
        other => {
            out.push_str(&emit_flow(other));
            out.push('\n');
        }
    }
    out
}

fn emit_map(out: &mut String, entries: &[(String, Value)], indent: usize) {
    for (key, value) in entries {
        if is_empty(value) {
            continue;
        }
        if let Value::Map(inner) = value {
            push_header(out, indent, key);
            emit_map(out, inner, indent.saturating_add(2));
        } else if let Value::List(items) = value {
            if items.iter().any(is_block_item) {
                push_header(out, indent, key);
                emit_list_block(out, items, indent.saturating_add(2));
            } else {
                push_scalar(out, indent, key, &emit_flow(value));
            }
        } else {
            push_scalar(out, indent, key, &emit_flow(value));
        }
    }
}

fn emit_list_block(out: &mut String, items: &[Value], indent: usize) {
    for item in items {
        if let Value::Map(entries) = item {
            for (position, (key, value)) in entries.iter().enumerate() {
                let prefix = if position == 0 { "- " } else { "  " };
                push_indent(out, indent);
                out.push_str(prefix);
                out.push_str(key);
                out.push_str(": ");
                out.push_str(&emit_flow(value));
                out.push('\n');
            }
        } else {
            push_indent(out, indent);
            out.push_str("- ");
            out.push_str(&emit_flow(item));
            out.push('\n');
        }
    }
}

fn emit_flow(value: &Value) -> String {
    match value {
        Value::Str(text) => {
            if is_plain(text) {
                text.clone()
            } else {
                quote(text)
            }
        }
        Value::Int(number) => number.to_string(),
        Value::Float(number) => format!("{number}"),
        Value::Bool(flag) => flag.to_string(),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(emit_flow).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|(key, value)| format!("{key}: {}", emit_flow(value)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

fn push_header(out: &mut String, indent: usize, key: &str) {
    push_indent(out, indent);
    out.push_str(key);
    out.push_str(":\n");
}

fn push_scalar(out: &mut String, indent: usize, key: &str, flow: &str) {
    push_indent(out, indent);
    out.push_str(key);
    out.push_str(": ");
    out.push_str(flow);
    out.push('\n');
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Map(entries) => entries.is_empty(),
        Value::List(items) => items.is_empty(),
        _ => false,
    }
}

fn is_block_item(value: &Value) -> bool {
    matches!(value, Value::Map(_) | Value::List(_))
}

fn looks_numeric(text: &str) -> bool {
    text.chars().any(|c| c == '.' || c == 'e' || c == 'E')
}

/// `true` se o texto pode sair **cru** (sem aspas) no TOON.
fn is_plain(text: &str) -> bool {
    if text.is_empty() || text == "-" {
        return false;
    }
    if text.trim() != text || text.starts_with("- ") {
        return false;
    }
    if text == "true" || text == "false" {
        return false;
    }
    if text.chars().any(char::is_control) {
        return false;
    }
    if text
        .chars()
        .any(|c| matches!(c, '"' | '\\' | '#' | ',' | '[' | ']' | '{' | '}'))
    {
        return false;
    }
    if text.parse::<i64>().is_ok() {
        return false;
    }
    if looks_numeric(text) && text.parse::<f64>().is_ok() {
        return false;
    }
    true
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() => out.extend(c.escape_default()),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests;
