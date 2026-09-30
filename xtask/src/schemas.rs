//! `check-schemas` (E06-T02) — linter dos esquemas de tools + registo do modelo.
//!
//! Falha o CI se algum esquema violar as regras (nomes, descrições que ensinam, enums fechados,
//! ids com `pattern`, anti-*poisoning*) ou se o catálogo/registo colunar driftar das tools.

use katu_core::toon::schema;
use katu_tools::schema::{SCHEMAS, catalog, lint_all};

/// Valida os esquemas das tools **e** o registo colunar/catálogo; devolve os problemas agregados.
pub(crate) fn check_schemas() -> Result<(), String> {
    let mut problems: Vec<String> = Vec::new();
    let issues = lint_all();
    if !issues.is_empty() {
        problems.push(issues.to_string());
    }
    for problem in schema::validate() {
        problems.push(format!("registo do modelo: {problem}"));
    }
    let cat = catalog();
    for tool in SCHEMAS {
        if !cat.contains(tool.name) {
            problems.push(format!("catálogo sem a tool `{}`", tool.name));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-schemas falhou:\n  {}",
            problems.join("\n  ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::check_schemas;

    #[test]
    fn real_schemas_pass() {
        assert!(check_schemas().is_ok());
    }
}
