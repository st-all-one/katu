//! `check-schemas` (E06-T02) — o linter de schema de tools falha o CI se algum esquema violar as
//! regras (nomes, descrições que ensinam, enums fechados, ids com `pattern`, anti-*poisoning*).

use katu_tools::schema::lint_all;

/// Valida todos os esquemas do registry; devolve os problemas **agregados**.
pub(crate) fn check_schemas() -> Result<(), String> {
    let issues = lint_all();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format!("check-schemas falhou:\n  {issues}"))
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
