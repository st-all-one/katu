//! `check-docs` (postmortems) — secções obrigatórias e guardrail com teste (E14-T02).
//!
//! Um postmortem sem guardrail é uma história, não uma correção: além das secções, exige-se uma
//! ligação a um **teste** (`.rs`).

use std::fs;
use std::path::Path;

use crate::docs::is_index;
use crate::markdown_links;
use crate::walk::collect_by_extension;

/// Secções obrigatórias de um postmortem.
const SECTIONS: [&str; 6] = [
    "## Executive summary",
    "## Impact",
    "## Timeline",
    "## Root cause",
    "## Guardrails added",
    "## Lessons",
];

/// Verifica os postmortems: secções obrigatórias e guardrail com teste.
///
/// # Erros
/// Mensagem agregada; `Ok(())` se `docs/postmortems/` ainda não existir.
pub(crate) fn check_postmortems() -> Result<(), String> {
    let dir = Path::new("docs/postmortems");
    if !dir.exists() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_by_extension(dir, "md", &mut files)?;
    files.sort();
    let mut violations: Vec<String> = Vec::new();
    for file in &files {
        if is_index(file) {
            continue;
        }
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        for section in SECTIONS {
            if !text.contains(section) {
                violations.push(format!("{}: falta `{section}`", file.display()));
            }
        }
        let has_test = markdown_links(&text).iter().any(|link| {
            let target = link.split('#').next().unwrap_or("");
            Path::new(target)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        });
        if !has_test {
            violations.push(format!(
                "{}: `## Guardrails added` sem teste (`.rs`)",
                file.display()
            ));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-docs falhou (postmortems; E14-T02):\n  {}",
            violations.join("\n  ")
        ))
    }
}
