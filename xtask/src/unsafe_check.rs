//! `check-unsafe` (E13-T04) — `#![forbid(unsafe_code)]` em cada crate puro, sem escape hatch.
//!
//! A política de memória (ADR 0016) é `forbid`, não `deny`: `unsafe` é um **erro de compilação**
//! em todo o crate puro. Este check confirma a declaração na raiz de cada crate e recusa
//! `allow(unsafe_code)` (a exceção seria uma decisão de crate, registada). O CI corre também Miri
//! (`cargo miri test`) e `cargo machete` (deps mortas).

use std::fs;
use std::path::{Path, PathBuf};

use crate::walk::{collect_by_extension, is_foreign_root};

/// Declaração de crate que torna `unsafe` um erro de compilação.
const DECLARATION: &str = "forbid(unsafe_code)";

/// Escape hatch que este check recusa em código puro.
const ESCAPE: &str = "allow(unsafe_code)";

/// Ponto de entrada de `check-unsafe`.
pub(crate) fn check_unsafe() -> Result<(), String> {
    let mut violations: Vec<String> = Vec::new();
    for root in crate_roots()? {
        if !crate_declares(&root) {
            violations.push(format!("{}: falta `#![{DECLARATION}]`", root.display()));
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        for (index, line) in text.lines().enumerate() {
            if is_escape(line) {
                violations.push(format!(
                    "{}:{}: `{ESCAPE}`",
                    file.display(),
                    index.saturating_add(1)
                ));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-unsafe falhou (ADR 0016):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// `true` se a raiz do crate declara `forbid(unsafe_code)` em `lib.rs`/`main.rs`.
fn crate_declares(root: &Path) -> bool {
    ["src/lib.rs", "src/main.rs"].iter().any(|relative| {
        fs::read_to_string(root.join(relative)).is_ok_and(|text| text.contains(DECLARATION))
    })
}

/// Diretórios de crate puro (não-foreign) sob `crates/`.
fn crate_roots() -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir("crates").map_err(|err| format!("lendo crates: {err}"))?;
    let mut roots: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !is_foreign_root(&path) && path.join("Cargo.toml").exists() {
            roots.push(path);
        }
    }
    roots.sort();
    Ok(roots)
}

/// `true` se a linha contém o escape hatch.
fn is_escape(line: &str) -> bool {
    line.contains(ESCAPE)
}

#[cfg(test)]
mod tests {
    use super::{DECLARATION, is_escape};

    #[test]
    fn declaration_and_escape_are_detected() {
        assert!("#![forbid(unsafe_code)]".contains(DECLARATION));
        assert!(!is_escape("forbid(unsafe_code)"));
        assert!(is_escape("    #[allow(unsafe_code)]"));
    }
}
