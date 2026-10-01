//! Projeção do [`Value`] no formato colunar D39 (ADR 0005, emenda v3).
//!
//! **Guiada pelo registo de esquema** ([`super::schema`]): como não há headers no *stream*, as
//! colunas de cada secção vêm do registo. Regras:
//! - escalares no topo → secção `k` (`k`,`v`) — explícitos, sem defaults;
//! - `List<Map>` → secção de linhas com as colunas do registo (key ausente = vazio);
//! - `List<escalar>` → secção de linhas de uma coluna (`ref`);
//! - `Block`/`List<Str>` sob chave literal → **bloco literal** (`\x1d`);
//! - lista dentro de uma linha → secção-filha `{pai}.{chave}` (linhas com `g`, índice da mãe).

use std::borrow::Cow;
use std::fmt::Write as _;

use super::Value;
use super::colunar::{Cell, RowTable, Section};
use super::schema::{self, Mode};
use crate::diag::{Level, events};

/// Lista aninhada: (secção-filha, índice da linha-mãe, itens **emprestados** do payload).
///
/// Os itens são emprestados em vez de clonados: a projeção antiga clonava cada lista aninhada
/// inteira (uma alocação por item e por chave) só para a emitir a seguir.
type Nested<'a> = Vec<(String, i64, &'a [Value])>;

/// Projeta um payload (árvore [`Value`]) num *stream* de secções.
#[must_use]
pub fn project(value: &Value) -> Vec<Section<'_>> {
    let _span = crate::fn_span!(Level::Trace, events::TOON_PROJECT, "toon::project::project");
    match value {
        Value::Map(entries) => project_map(entries),
        other => vec![Section::Rows({
            let mut table = RowTable::new("k");
            table.push(vec![Cell::text("v"), cell(other)]);
            table
        })],
    }
}

fn project_map(entries: &[(String, Value)]) -> Vec<Section<'_>> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::TOON_PROJECT,
        "toon::project::project_map"
    );
    let mut sections = Vec::new();
    let scalars: Vec<&(String, Value)> = entries
        .iter()
        .filter(|(_, value)| is_scalar(value))
        .collect();
    if !scalars.is_empty() {
        let mut table = RowTable::new("k");
        for (key, value) in scalars {
            table.push(vec![Cell::text(key.clone()), cell(value)]);
        }
        sections.push(Section::Rows(table));
    }
    for (key, value) in entries {
        if is_scalar(value) {
            continue;
        }
        match value {
            Value::List(items) => sections.extend(list_sections(key, items)),
            Value::Block(text) => sections.push(Section::literal(key, text)),
            Value::Map(_) | Value::Flow(_) => sections.extend(one_row(key, value)),
            _ => {}
        }
    }
    sections
}

/// Escalares que caem diretamente numa célula.
fn is_scalar(value: &Value) -> bool {
    let _span = crate::trace_fn!("toon::project::is_scalar");

    matches!(
        value,
        Value::Str(_) | Value::Int(_) | Value::Float(_) | Value::Bool(_)
    )
}

fn cell(value: &Value) -> Cell<'_> {
    let _span = crate::trace_fn!("toon::project::cell");

    match value {
        Value::Str(text) | Value::Block(text) => Cell::Text(Cow::Borrowed(text)),
        Value::Int(number) => Cell::int(*number),
        Value::Bool(flag) => Cell::bool(*flag),
        Value::Float(number) => Cell::Text(Cow::Owned(format!("{number}"))),
        Value::List(_) | Value::Map(_) | Value::Flow(_) => Cell::Text(Cow::Owned(flatten(value))),
    }
}

fn text_of(value: &Value) -> Cow<'_, str> {
    let _span = crate::trace_fn!("toon::project::text_of");

    match value {
        Value::Str(text) | Value::Block(text) => Cow::Borrowed(text),
        other => Cow::Owned(flatten(other)),
    }
}

fn flatten(value: &Value) -> String {
    let _span = crate::trace_fn!("toon::project::flatten");

    let mut out = String::new();
    flatten_into(&mut out, value);
    out
}

/// Achata um valor numa célula, escrevendo **direto** no buffer (sem `Vec<String>` + `join`).
fn flatten_into(out: &mut String, value: &Value) {
    let _span = crate::trace_fn!("toon::project::flatten_into");

    match value {
        Value::Str(text) | Value::Block(text) => out.push_str(text),
        Value::Int(number) => {
            write!(out, "{number}").unwrap_or_default();
        }
        Value::Bool(flag) => out.push(if *flag { '1' } else { '0' }),
        Value::Float(number) => {
            write!(out, "{number}").unwrap_or_default();
        }
        Value::List(items) => {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                flatten_into(out, item);
            }
        }
        Value::Map(entries) | Value::Flow(entries) => {
            for (index, (name, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(name);
                out.push('=');
                flatten_into(out, item);
            }
        }
    }
}

fn list_sections<'a>(name: &str, items: &'a [Value]) -> Vec<Section<'a>> {
    let _span = crate::trace_fn!("toon::project::list_sections");

    if is_literal(name) {
        let lines: Vec<Cow<'a, str>> = items.iter().map(text_of).collect();
        return vec![Section::Literal {
            name: Cow::Owned(name.to_string()),
            lines,
        }];
    }
    let mut sections = Vec::new();
    let (table, nested) = build_rows(name, items);
    sections.push(Section::Rows(table));
    sections.extend(child_sections(&nested));
    sections
}

fn is_literal(name: &str) -> bool {
    let _span = crate::trace_fn!("toon::project::is_literal");

    schema::spec(name).is_some_and(|spec| spec.mode == Mode::Literal)
}

/// Colunas do registo para a secção (vazio = união das chaves).
fn columns(name: &str, items: &[Value]) -> Vec<String> {
    let _span = crate::trace_fn!("toon::project::columns");

    if let Some(spec) = schema::spec(name)
        && !spec.cols.is_empty()
    {
        return spec.cols.iter().map(|col| col.name.to_string()).collect();
    }
    union_names(items)
}

fn build_rows<'a>(name: &str, items: &'a [Value]) -> (RowTable<'a>, Nested<'a>) {
    let _span = crate::fn_span!(
        Level::Trace,
        events::TOON_PROJECT,
        "toon::project::build_rows"
    );
    let cols = columns(name, items);
    let mut table = RowTable::new(name.to_string());
    let mut nested = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let position = i64::try_from(index).unwrap_or(i64::MAX);
        let mut row = Vec::with_capacity(cols.len());
        match item {
            Value::Map(entries) => {
                for col in &cols {
                    match lookup(entries, col) {
                        Some(Value::List(sub)) => {
                            nested.push((format!("{name}.{col}"), position, sub.as_slice()));
                            row.push(Cell::text(""));
                        }
                        Some(value) => row.push(cell(value)),
                        None => row.push(Cell::text("")),
                    }
                }
                // Chaves fora do esquema que sejam listas → secções-filha (ex.: `hits`, `lines`).
                for (key, value) in entries {
                    if !cols.contains(key)
                        && let Value::List(sub) = value
                    {
                        nested.push((format!("{name}.{key}"), position, sub.as_slice()));
                    }
                }
            }
            other => row.push(cell(other)),
        }
        table.push(row);
    }
    (table, nested)
}

fn child_sections<'a>(nested: &Nested<'a>) -> Vec<Section<'a>> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::TOON_PROJECT,
        "toon::project::child_sections"
    );
    let mut order: Vec<&str> = Vec::new();
    for (name, _, _) in nested {
        if !order.contains(&name.as_str()) {
            order.push(name.as_str());
        }
    }
    let mut sections = Vec::with_capacity(order.len());
    for name in order {
        let rows: Vec<(i64, &'a Value)> = nested
            .iter()
            .filter(|(key, _, _)| key == name)
            .flat_map(|(_, group, items)| items.iter().map(move |item| (*group, item)))
            .collect();
        if is_literal(name) {
            sections.push(Section::Literal {
                name: Cow::Owned(name.to_string()),
                lines: rows.iter().map(|(_, item)| text_of(item)).collect(),
            });
            continue;
        }
        let cols: Vec<String> = schema::spec(name)
            .map(|spec| spec.cols.iter().map(|col| col.name.to_string()).collect())
            .unwrap_or_default();
        let mut table = RowTable::new(name.to_string());
        for (group, item) in &rows {
            let mut row = Vec::with_capacity(cols.len());
            for col in &cols {
                if col == "g" {
                    row.push(Cell::int(*group));
                    continue;
                }
                let value = match item {
                    Value::Map(entries) => lookup(entries, col),
                    _ => None,
                };
                row.push(value.map_or_else(|| Cell::text(""), cell));
            }
            table.push(row);
        }
        sections.push(Section::Rows(table));
    }
    sections
}

/// Uma linha a partir de um mapa do topo (o `Value` é **emprestado** — nada se clona).
fn one_row<'a>(name: &str, value: &'a Value) -> Vec<Section<'a>> {
    let _span = crate::trace_fn!("toon::project::one_row");

    let mut sections = Vec::new();
    let (table, nested) = build_rows(name, std::slice::from_ref(value));
    sections.push(Section::Rows(table));
    sections.extend(child_sections(&nested));
    sections
}

fn union_names(items: &[Value]) -> Vec<String> {
    let _span = crate::trace_fn!("toon::project::union_names");

    let mut names: Vec<String> = Vec::new();
    for item in items {
        if let Value::Map(entries) = item {
            for (name, _) in entries {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
    }
    names
}

fn lookup<'a>(entries: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    let _span = crate::trace_fn!("toon::project::lookup");

    entries
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}
