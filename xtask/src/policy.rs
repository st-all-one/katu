//! `policy:audit` (E02-T04) — lista `Enforced` vs `Advisory` e falha se uma regra `Enforced` não
//! tiver exemplo negativo, não **ensinar o que passaria** (Q-08) ou souber a colisão de
//! `id`/enunciado vazio.

use std::fs;
use std::path::{Path, PathBuf};

use katu_policy::{RuleCategory, RuleSet, audit};

use crate::walk::collect_rule_files;

/// Pasta default dos artefactos de política.
const POLICY_DIR: &str = "policy";

/// Audita todos os `policy/*.toml` (ou os caminhos passados em `args`).
pub(crate) fn policy_audit(args: &[String]) -> Result<(), String> {
    let files = policy_files(args)?;
    if files.is_empty() {
        return Err(format!(
            "policy:audit: nenhum ficheiro de política em {POLICY_DIR}/*.toml"
        ));
    }
    let mut violations: Vec<String> = Vec::new();
    let mut enforced = 0usize;
    let mut advisory = 0usize;
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let rules =
            RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        let report = audit(&rules, 0);
        // Q-08: uma regra que nega sem dizer o que passaria deixa o modelo a tentar às cegas.
        for rule in &rules.rules {
            if rule.category == RuleCategory::Enforced
                && rule.remedy.as_deref().is_none_or(str::is_empty)
            {
                violations.push(format!(
                    "{}: regra Enforced `{}` sem `remedy` (Q-08)",
                    file.display(),
                    rule.id.as_str()
                ));
            }
        }
        enforced = enforced.saturating_add(report.enforced.len());
        advisory = advisory.saturating_add(report.advisory.len());
        for issue in &report.issues {
            violations.push(format!("{}: {issue}", file.display()));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "policy:audit falhou ({enforced} Enforced, {advisory} Advisory):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Ficheiros de política: os passados em `args` ou todos os `*.toml` de `policy/`.
fn policy_files(args: &[String]) -> Result<Vec<PathBuf>, String> {
    if !args.is_empty() {
        return Ok(args.iter().map(PathBuf::from).collect());
    }
    let mut files = Vec::new();
    collect_rule_files(Path::new(POLICY_DIR), &mut files)?;
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::policy_files;

    #[test]
    fn explicit_args_are_used_verbatim() {
        let args = vec!["policy/a.toml".to_string(), "policy/b.toml".to_string()];
        let files = policy_files(&args).unwrap_or_default();
        assert_eq!(files.len(), 2);
    }
}
