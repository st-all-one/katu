//! `grep`: conteúdo com classificação e clusterização por símbolo (E06-T05).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use katu_core::diag::{Level, events};
use katu_core::ports::Fs;
use katu_core::report::{ToolReport, content_id};
use katu_core::toon::Value;

use crate::lang::{len_u64, to_i64};
use crate::outline::{Symbol, outline};

use super::walk;

/// Limite de ficheiros varridos (evita picos em repositórios enormes).
const MAX_FILES: usize = 4096;

/// Um hit classificado.
pub(super) struct Hit {
    /// Caminho.
    pub path: String,
    /// Linha (1-based).
    pub line: u32,
    /// Tipo de linha.
    pub kind: &'static str,
    /// Pré-visualização.
    pub preview: String,
    /// Símbolo que contém a linha.
    pub symbol: Option<String>,
}

/// Origem do ficheiro (para classificar a linha).
#[derive(Clone, Copy)]
enum Origin {
    /// Ficheiro de teste.
    Test,
    /// Código de produção.
    Source,
}

pub(super) fn run(fs: &dyn Fs, root: &Path, query: &str, limit: usize) -> ToolReport {
    let _span = katu_core::fn_span!(Level::Debug, events::TOOL_SEARCH, "search::grep::run");
    let files = walk::walk(fs, root, MAX_FILES);
    let mut hits = Vec::new();
    let mut scanned = 0_usize;
    'outer: for file in &files {
        scanned = scanned.saturating_add(1);
        let Ok(bytes) = fs.read(file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let symbols = outline(&text);
        let origin = if text.contains("#[test]") || text.contains("#[cfg(test)]") {
            Origin::Test
        } else {
            Origin::Source
        };
        for (index, line) in text.lines().enumerate() {
            if !line.contains(query) {
                continue;
            }
            let number = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            hits.push(Hit {
                path: file.display().to_string(),
                line: number,
                kind: classify(line, origin),
                preview: preview(line),
                symbol: enclosing(&symbols, number),
            });
            if hits.len() >= limit {
                break 'outer;
            }
        }
    }
    build(root, query, &files, scanned, &hits)
}

fn build(root: &Path, query: &str, files: &[PathBuf], scanned: usize, hits: &[Hit]) -> ToolReport {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_SEARCH, "search::grep::build");
    let mut clusters: BTreeMap<String, Vec<&Hit>> = BTreeMap::new();
    for hit in hits {
        let key = hit
            .symbol
            .clone()
            .unwrap_or_else(|| "(top-level)".to_string());
        clusters.entry(key).or_default().push(hit);
    }
    let groups: Vec<Value> = clusters
        .iter()
        .map(|(name, items)| cluster_value(name, items))
        .collect();
    let mut with_hits: BTreeSet<&str> = BTreeSet::new();
    for hit in hits {
        with_hits.insert(hit.path.as_str());
    }
    let without = files.len().saturating_sub(with_hits.len());
    let data = Value::map(vec![
        ("query".to_string(), Value::str(query)),
        ("root".to_string(), Value::str(root.display().to_string())),
        ("scanned".to_string(), Value::int(to_i64(len_u64(scanned)))),
        (
            "searched".to_string(),
            Value::int(to_i64(len_u64(files.len()))),
        ),
        ("hits".to_string(), Value::int(to_i64(len_u64(hits.len())))),
        ("clusters".to_string(), Value::list(groups)),
        (
            "negative".to_string(),
            Value::str(format!("{without} ficheiros sem correspondência")),
        ),
    ]);
    let id = content_id("q", format!("grep:{query}").as_bytes());
    let next = clusters
        .keys()
        .next()
        .map(|name| vec![format!("read {id}#{name}")])
        .unwrap_or_default();
    ToolReport::new("search.grep", data)
        .with_id(id)
        .with_next(next)
}

fn cluster_value(name: &str, items: &[&Hit]) -> Value {
    let _span = katu_core::trace_fn!("search::grep::cluster_value");

    let symbol = content_id("s", name.as_bytes());
    let hits: Vec<Value> = items.iter().map(|hit| hit_value(hit)).collect();
    Value::map(vec![
        ("sym".to_string(), Value::str(symbol)),
        ("name".to_string(), Value::str(name)),
        ("hits".to_string(), Value::list(hits)),
    ])
}

fn hit_value(hit: &Hit) -> Value {
    let _span = katu_core::trace_fn!("search::grep::hit_value");

    let mut entries = vec![
        ("path".to_string(), Value::str(hit.path.clone())),
        ("ln".to_string(), Value::int(i64::from(hit.line))),
        ("ty".to_string(), Value::str(hit.kind)),
        ("preview".to_string(), Value::str(hit.preview.clone())),
    ];
    if let Some(symbol) = &hit.symbol {
        entries.push(("sym".to_string(), Value::str(symbol.clone())));
    }
    Value::map(entries)
}

fn classify(line: &str, origin: Origin) -> &'static str {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_SEARCH, "search::grep::classify");
    let trimmed = line.trim_start();
    if matches!(origin, Origin::Test) && trimmed.contains("fn ") {
        return "test";
    }
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        return "comment";
    }
    if trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("#include")
    {
        return "import";
    }
    if line.contains('"') {
        return "string";
    }
    "code"
}

fn preview(line: &str) -> String {
    const MAX: usize = 120;
    let trimmed = line.trim();
    let mut out: String = trimmed.chars().take(MAX).collect();
    if trimmed.chars().count() > MAX {
        out.push('…');
    }
    out
}

/// Símbolo mais **interior** que contém a linha.
fn enclosing(symbols: &[Symbol], line: u32) -> Option<String> {
    let _span = katu_core::trace_fn!("search::grep::enclosing");

    symbols
        .iter()
        .filter(|symbol| symbol.start <= line && line <= symbol.end)
        .min_by_key(|symbol| symbol.end.saturating_sub(symbol.start))
        .map(|symbol| symbol.name.clone())
}
