//! `check-rule-coverage` (E13-T02) — matriz regra `Enforced` ↔ teste de caminho real.
//!
//! Cada regra `Enforced` de `policy/*.toml` tem de apontar, em `coverage.toml`, para um teste que
//! conduz o caminho real e asserta a decisão. O check verifica a **totalidade** da matriz (nenhuma
//! regra sem teste, nenhuma entrada órfã) e que o teste nomeado existe.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use katu_policy::{RuleCategory, RuleSet};
use serde::Deserialize;

use crate::walk::{collect_by_extension, collect_rule_files};

/// Matriz versionada.
const COVERAGE_FILE: &str = "coverage.toml";

/// Matriz regra ↔ teste.
#[derive(Debug, Deserialize)]
struct Coverage {
    #[serde(default)]
    rules: Vec<CoverageEntry>,
}

/// Entrada da matriz.
#[derive(Debug, Deserialize)]
struct CoverageEntry {
    /// Id da regra `Enforced`.
    id: String,
    /// Nome do teste (`fn <test>`).
    test: String,
}

/// Ponto de entrada de `check-rule-coverage`.
pub(crate) fn check_rule_coverage() -> Result<(), String> {
    let text =
        fs::read_to_string(COVERAGE_FILE).map_err(|err| format!("lendo {COVERAGE_FILE}: {err}"))?;
    let coverage: Coverage =
        toml::from_str(&text).map_err(|err| format!("{COVERAGE_FILE}: {err}"))?;
    let enforced = enforced_rule_ids()?;
    let sources = test_sources()?;
    let mut violations: Vec<String> = Vec::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for entry in &coverage.rules {
        if !enforced.contains(&entry.id) {
            violations.push(format!("{}: regra inexistente ou não-Enforced", entry.id));
        }
        covered.insert(entry.id.clone());
        if !sources.contains(&format!("fn {}(", entry.test)) {
            violations.push(format!(
                "{}: teste `{}` não encontrado nas fontes",
                entry.id, entry.test
            ));
        }
    }
    for id in enforced.difference(&covered) {
        violations.push(format!("{id}: regra Enforced sem teste na matriz"));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-rule-coverage falhou (E13-T02):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Ids das regras `Enforced` de `policy/*.toml`.
fn enforced_rule_ids() -> Result<BTreeSet<String>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rule_files(Path::new("policy"), &mut files)?;
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let set = RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        for rule in &set.rules {
            if rule.category == RuleCategory::Enforced {
                ids.insert(rule.id.as_str().to_string());
            }
        }
    }
    Ok(ids)
}

/// Concatenação de todas as fontes `.rs` do workspace (procura de `fn <test>`).
fn test_sources() -> Result<String, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    let mut sources = String::new();
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        sources.push_str(&text);
        sources.push('\n');
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::Coverage;

    #[test]
    fn coverage_parses() -> Result<(), Box<dyn std::error::Error>> {
        let coverage: Coverage = toml::from_str("[[rules]]\nid = \"r\"\ntest = \"t\"\n")?;
        assert_eq!(coverage.rules.len(), 1);
        Ok(())
    }
}
