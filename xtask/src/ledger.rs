//! `ledger:validate` (E02-T06) — o ledger de cobertura de regras é consistente com `policy/`:
//! `coverage_id` únicos, estados válidos, e cada regra `Enforced` exatamente uma vez coberta.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use katu_policy::{RuleCategory, RuleSet};

use crate::walk::collect_by_extension;

/// Ledger de cobertura versionado.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    version: u32,
    entries: Vec<LedgerEntry>,
}

/// Entrada do ledger: uma superfície coberta ou explicitamente não aplicável/deferida.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerEntry {
    coverage_id: String,
    state: LedgerState,
    #[serde(default)]
    rule_id: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// Estado de cobertura de uma superfície.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LedgerState {
    /// Coberta por uma regra.
    Covered,
    /// Não aplicável (com motivo).
    NotApplicable,
    /// Adiada (com motivo).
    Deferred,
}

impl LedgerState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::NotApplicable => "not_applicable",
            Self::Deferred => "deferred",
        }
    }
}

/// Ledger default.
const DEFAULT_LEDGER: &str = "policy/coverage-ledger.json";

/// Pasta dos artefactos de política.
const POLICY_DIR: &str = "policy";

/// Valida o ledger contra as regras `Enforced` dos `policy/*.toml`.
pub(crate) fn ledger_validate(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .map_or_else(|| PathBuf::from(DEFAULT_LEDGER), PathBuf::from);
    let text =
        fs::read_to_string(&path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
    let ledger: Ledger = serde_json::from_str(&text)
        .map_err(|err| format!("{}: json inválido: {err}", path.display()))?;
    let enforced = enforced_rule_ids()?;
    let violations = validate(&ledger, &enforced);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "ledger:validate falhou:\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Regras `Enforced` presentes nos `*.toml` de `policy/`.
fn enforced_rule_ids() -> Result<BTreeSet<String>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new(POLICY_DIR), "toml", &mut files)?;
    let mut ids = BTreeSet::new();
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let rules =
            RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        for rule in &rules.rules {
            if rule.category == RuleCategory::Enforced {
                ids.insert(rule.id.as_str().to_string());
            }
        }
    }
    Ok(ids)
}

/// Validação pura (testável sem sistema de ficheiros).
fn validate(ledger: &Ledger, enforced: &BTreeSet<String>) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    if ledger.version != 1 {
        violations.push(format!(
            "versão do ledger não suportada: {}",
            ledger.version
        ));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for entry in &ledger.entries {
        if entry.coverage_id.trim().is_empty() {
            violations.push("coverage_id vazio".to_string());
        }
        if !seen.insert(entry.coverage_id.clone()) {
            violations.push(format!("coverage_id duplicado: {}", entry.coverage_id));
        }
        match entry.state {
            LedgerState::Covered => match entry.rule_id.as_deref() {
                Some(id) if !id.trim().is_empty() => {
                    covered.insert(id.to_string());
                }
                _ => violations.push(format!(
                    "entrada `covered` sem rule_id: {}",
                    entry.coverage_id
                )),
            },
            LedgerState::NotApplicable | LedgerState::Deferred => {
                if entry
                    .reason
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
                {
                    violations.push(format!(
                        "entrada `{}` sem reason: {}",
                        entry.state.as_str(),
                        entry.coverage_id
                    ));
                }
            }
        }
    }
    for id in enforced {
        if !covered.contains(id) {
            violations.push(format!("regra Enforced sem entrada `covered`: {id}"));
        }
    }
    for id in &covered {
        if !enforced.contains(id) {
            violations.push(format!("ledger cobre uma regra inexistente: {id}"));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::{Ledger, LedgerEntry, LedgerState, validate};
    use std::collections::BTreeSet;

    fn entry(coverage_id: &str, state: LedgerState, rule_id: Option<&str>) -> LedgerEntry {
        LedgerEntry {
            coverage_id: coverage_id.to_string(),
            state,
            rule_id: rule_id.map(str::to_string),
            reason: None,
        }
    }

    #[test]
    fn covered_must_match_enforced_rules() {
        let enforced: BTreeSet<String> = BTreeSet::from(["r1".to_string()]);
        let ledger = Ledger {
            version: 1,
            entries: vec![entry("k1", LedgerState::Covered, Some("r1"))],
        };
        assert!(validate(&ledger, &enforced).is_empty());
    }

    #[test]
    fn duplicate_and_missing_reason_are_flagged() {
        let enforced: BTreeSet<String> = BTreeSet::new();
        let ledger = Ledger {
            version: 1,
            entries: vec![
                entry("dup", LedgerState::Covered, Some("r1")),
                entry("dup", LedgerState::Deferred, None),
            ],
        };
        let violations = validate(&ledger, &enforced);
        assert!(violations.iter().any(|v| v.contains("duplicado")));
        assert!(violations.iter().any(|v| v.contains("sem reason")));
    }

    #[test]
    fn uncovered_enforced_rule_is_flagged() {
        let enforced: BTreeSet<String> = BTreeSet::from(["r9".to_string()]);
        let ledger = Ledger {
            version: 1,
            entries: Vec::new(),
        };
        let violations = validate(&ledger, &enforced);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("sem entrada `covered`"))
        );
    }
}
