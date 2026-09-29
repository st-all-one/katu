//! Índice append-only da lixeira (`<root>/.katu/trash/index.tsv`).
//!
//! Formato determinístico por linha: `at_millis \t stored \t original`, com `\`, `\t` e `\n`
//! escapados. Apenas se **anexa** (nunca se reescreve) — a lixeira é recuperável e auditável.

use std::path::{Path, PathBuf};

use katu_core::ports::{Fs, FsError};

/// Um registo da lixeira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrashRecord {
    /// Instante (ms) em que foi movido.
    pub(super) at_millis: u64,
    /// Caminho guardado (na lixeira).
    pub(super) stored: String,
    /// Caminho original.
    pub(super) original: String,
}

/// Caminho do índice.
#[must_use]
pub(super) fn index_path(root: &Path) -> PathBuf {
    root.join(".katu").join("trash").join("index.tsv")
}

/// Lê o índice (vazio se ainda não existir).
#[must_use]
pub(super) fn read(fs: &dyn Fs, root: &Path) -> Vec<TrashRecord> {
    let Ok(bytes) = fs.read(&index_path(root)) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(parse)
        .collect()
}

/// Anexa um registo ao índice.
pub(super) fn append(fs: &dyn Fs, root: &Path, record: &TrashRecord) -> Result<(), FsError> {
    let stored = encode(&record.stored);
    let original = encode(&record.original);
    let at = record.at_millis;
    let line = format!("{at}\t{stored}\t{original}\n");
    fs.append(&index_path(root), line.as_bytes())
}

fn parse(line: &str) -> Option<TrashRecord> {
    let mut parts = line.split('\t');
    let at_millis = parts.next()?.parse::<u64>().ok()?;
    let stored = decode(parts.next()?);
    let original = decode(parts.next()?);
    Some(TrashRecord {
        at_millis,
        stored,
        original,
    })
}

fn encode(field: &str) -> String {
    field
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn decode(field: &str) -> String {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        if let Some(marker) = chars.next() {
            match marker {
                't' => out.push('\t'),
                'n' => out.push('\n'),
                _ => out.push(marker),
            }
        }
    }
    out
}
