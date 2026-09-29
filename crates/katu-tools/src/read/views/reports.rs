//! Construtores de relatório por view (`full`/`range`/`outline`/`summary`/`symbol`).

use katu_core::report::{Page, ToolReport, content_id};
use katu_core::toon::Value;

use crate::outline::{Symbol, outline};
use crate::read::{LineRange, ReadBudget};

use super::Meta;
use super::helpers::{clip, flags, imports, language, len_u64, slice, to_i64};

pub(super) fn full(lines: &[&str], meta: &Meta<'_>, budget: ReadBudget) -> ToolReport {
    let (text, truncated) = clip(lines, budget);
    let shown = len_u64(text.lines().count());
    let page = Page {
        cursor: truncated.then_some(shown.saturating_add(1)),
        total: meta.loc,
        truncated,
    };
    let mut next = Vec::new();
    if truncated {
        next.push(format!("read {}@{}", meta.id, shown.saturating_add(1)));
    }
    let data = Value::map(vec![
        ("path".to_string(), Value::str(meta.path)),
        ("lang".to_string(), Value::str(language(meta.path))),
        ("loc".to_string(), Value::int(to_i64(meta.loc))),
        ("text".to_string(), Value::block(text)),
    ]);
    ToolReport::new("read.full", data)
        .with_id(meta.id.clone())
        .with_hash(meta.hash.clone())
        .with_page(page)
        .with_next(next)
}

pub(super) fn range_report(lines: &[&str], meta: &Meta<'_>, span: LineRange) -> ToolReport {
    let last = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    let start = span.start.max(1);
    let end = span.end.min(last).max(start);
    let body = slice(lines, start, end).join("\n");
    let data = Value::map(vec![
        ("path".to_string(), Value::str(meta.path)),
        (
            "range".to_string(),
            Value::list(vec![
                Value::int(i64::from(start)),
                Value::int(i64::from(end)),
            ]),
        ),
        ("text".to_string(), Value::block(body)),
    ]);
    ToolReport::new("read.range", data)
        .with_id(meta.id.clone())
        .with_hash(meta.hash.clone())
}

pub(super) fn outline_report(lines: &[&str], meta: &Meta<'_>) -> ToolReport {
    let data = Value::map(vec![
        ("path".to_string(), Value::str(meta.path)),
        ("loc".to_string(), Value::int(to_i64(meta.loc))),
        ("symbols".to_string(), symbols_value(meta.path, lines)),
    ]);
    ToolReport::new("read.outline", data)
        .with_id(meta.id.clone())
        .with_hash(meta.hash.clone())
}

pub(super) fn summary(lines: &[&str], meta: &Meta<'_>) -> ToolReport {
    let symbols = outline(&lines.join("\n"));
    let mut entries = vec![
        ("path".to_string(), Value::str(meta.path)),
        ("lang".to_string(), Value::str(language(meta.path))),
        ("loc".to_string(), Value::int(to_i64(meta.loc))),
    ];
    let imports = imports(lines);
    if !imports.is_empty() {
        entries.push((
            "imports".to_string(),
            Value::list(
                imports
                    .iter()
                    .map(|item| Value::str(item.clone()))
                    .collect(),
            ),
        ));
    }
    if !symbols.is_empty() {
        entries.push(("symbols".to_string(), symbols_value(meta.path, lines)));
    }
    let flags = flags(lines);
    if !flags.is_empty() {
        entries.push((
            "flags".to_string(),
            Value::list(
                flags
                    .iter()
                    .map(|(line, kind)| {
                        Value::map(vec![
                            ("line".to_string(), Value::int(i64::from(*line))),
                            ("kind".to_string(), Value::str(*kind)),
                        ])
                    })
                    .collect(),
            ),
        ));
    }
    let next = symbols
        .iter()
        .take(3)
        .map(|symbol| format!("read {}#{}", meta.id, symbol.name))
        .collect();
    ToolReport::new("read.summary", Value::map(entries))
        .with_id(meta.id.clone())
        .with_hash(meta.hash.clone())
        .with_next(next)
}

pub(super) fn symbol(lines: &[&str], meta: &Meta<'_>, wanted: &str) -> ToolReport {
    let symbols = outline(&lines.join("\n"));
    if let Some(found) = symbols.iter().find(|symbol| symbol.name == wanted) {
        let body = slice(lines, found.start, found.end).join("\n");
        let data = Value::map(vec![
            ("path".to_string(), Value::str(meta.path)),
            ("symbol".to_string(), Value::str(found.name.clone())),
            ("kind".to_string(), Value::str(found.kind.as_str())),
            (
                "range".to_string(),
                Value::list(vec![
                    Value::int(i64::from(found.start)),
                    Value::int(i64::from(found.end)),
                ]),
            ),
            ("text".to_string(), Value::block(body)),
        ]);
        return ToolReport::new("read.symbol", data)
            .with_id(meta.id.clone())
            .with_hash(meta.hash.clone());
    }
    let data = Value::map(vec![
        ("path".to_string(), Value::str(meta.path)),
        ("symbol".to_string(), Value::str(wanted)),
        ("found".to_string(), Value::bool(false)),
        ("symbols".to_string(), symbols_value(meta.path, lines)),
    ]);
    ToolReport::new("read.symbol", data)
        .with_id(meta.id.clone())
        .with_hash(meta.hash.clone())
}

fn symbols_value(path: &str, lines: &[&str]) -> Value {
    let symbols: Vec<Value> = outline(&lines.join("\n"))
        .iter()
        .map(|symbol| symbol_value(path, symbol))
        .collect();
    Value::list(symbols)
}

fn symbol_value(path: &str, symbol: &Symbol) -> Value {
    let seed = format!("{path}:{}:{}", symbol.name, symbol.start);
    Value::map(vec![
        (
            "id".to_string(),
            Value::str(content_id("s", seed.as_bytes())),
        ),
        ("kind".to_string(), Value::str(symbol.kind.as_str())),
        ("name".to_string(), Value::str(symbol.name.clone())),
        (
            "range".to_string(),
            Value::list(vec![
                Value::int(i64::from(symbol.start)),
                Value::int(i64::from(symbol.end)),
            ]),
        ),
    ])
}
