//! `find` (nomes por relevância) e `ls` (mapa semântico) (E06-T05).

use std::path::Path;

use katu_core::ports::Fs;
use katu_core::report::{ToolReport, content_id};
use katu_core::toon::Value;

use crate::lang::{language, len_u64, to_i64};
use crate::outline::outline;

use super::walk;

/// Limite de ficheiros varridos.
const MAX_FILES: usize = 4096;

pub(super) fn find(fs: &dyn Fs, root: &Path, query: &str, limit: usize) -> ToolReport {
    let files = walk::walk(fs, root, MAX_FILES);
    let needle = query.to_lowercase();
    let mut ranked: Vec<(u8, String)> = files
        .iter()
        .filter_map(|file| {
            let path = file.display().to_string();
            let name = file.file_name()?.to_str()?.to_lowercase();
            let score = rank(&name, &path.to_lowercase(), &needle);
            (score > 0).then_some((score, path))
        })
        .collect();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    ranked.truncate(limit);
    let items: Vec<Value> = ranked
        .iter()
        .map(|(score, path)| {
            Value::map(vec![
                ("path".to_string(), Value::str(path.clone())),
                ("score".to_string(), Value::int(i64::from(*score))),
            ])
        })
        .collect();
    let id = content_id("q", format!("find:{query}").as_bytes());
    let data = Value::map(vec![
        ("query".to_string(), Value::str(query)),
        (
            "matches".to_string(),
            Value::int(to_i64(len_u64(items.len()))),
        ),
        ("files".to_string(), Value::list(items)),
    ]);
    ToolReport::new("search.find", data).with_id(id)
}

fn rank(name: &str, path: &str, needle: &str) -> u8 {
    if needle.is_empty() {
        0
    } else if name == needle {
        3
    } else if name.contains(needle) {
        2
    } else {
        u8::from(path.contains(needle))
    }
}

pub(super) fn ls(fs: &dyn Fs, root: &Path, limit: usize) -> ToolReport {
    let entries = fs.list_dir(root).unwrap_or_default();
    let items: Vec<Value> = entries
        .iter()
        .take(limit)
        .map(|entry| entry_value(fs, entry))
        .collect();
    let id = content_id("q", format!("ls:{}", root.display()).as_bytes());
    let data = Value::map(vec![
        ("root".to_string(), Value::str(root.display().to_string())),
        ("entries".to_string(), Value::list(items)),
    ]);
    ToolReport::new("search.ls", data).with_id(id)
}

fn entry_value(fs: &dyn Fs, entry: &Path) -> Value {
    if fs.is_dir(entry) {
        return Value::map(vec![
            ("path".to_string(), Value::str(entry.display().to_string())),
            ("ty".to_string(), Value::str("dir")),
        ]);
    }
    let text = fs.read(entry).map_or_else(
        |_| String::new(),
        |bytes| String::from_utf8_lossy(&bytes).to_string(),
    );
    let symbols = outline(&text);
    let exports = text
        .lines()
        .filter(|line| line.trim_start().starts_with("pub "))
        .count();
    let tests = text.contains("#[test]") || text.contains("#[cfg(test)]");
    let shown = entry.to_string_lossy();
    Value::map(vec![
        ("path".to_string(), Value::str(shown.to_string())),
        ("ty".to_string(), Value::str("file")),
        ("lang".to_string(), Value::str(language(&shown))),
        (
            "loc".to_string(),
            Value::int(to_i64(len_u64(text.lines().count()))),
        ),
        (
            "syms".to_string(),
            Value::int(to_i64(len_u64(symbols.len()))),
        ),
        ("exports".to_string(), Value::int(to_i64(len_u64(exports)))),
        ("tests".to_string(), Value::bool(tests)),
    ])
}
