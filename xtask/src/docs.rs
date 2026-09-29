//! `check-docs` — ADRs com alternativas obrigatórias (E14-T01).
//!
//! Uma decisão sem o que **venceu** convida a re-litigá-la (§44). Este check falha se uma ADR em
//! `docs/adr/` não tiver a secção `## Alternatives considered`.

use std::fs;
use std::path::Path;

use crate::walk::collect_by_extension;

/// Secção obrigatória em cada ADR.
const REQUIRED_SECTION: &str = "## Alternatives considered";

/// Verifica as secções obrigatórias de todas as ADRs.
///
/// # Erros
/// Mensagem agregada com todas as ADRs em falta; `Ok(())` se `docs/adr/` ainda não existir.
pub(crate) fn check_adrs() -> Result<(), String> {
    let dir = Path::new("docs/adr");
    if !dir.exists() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_by_extension(dir, "md", &mut files)?;
    let mut violations: Vec<String> = Vec::new();
    for file in files {
        if is_index(&file) {
            continue;
        }
        let source =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        if !source.contains(REQUIRED_SECTION) {
            violations.push(format!("{}: falta `{REQUIRED_SECTION}`", file.display()));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-docs falhou (ADR sem alternativas; E14-T01):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// `true` para o índice/template (`README.md`), que não é uma decisão.
fn is_index(file: &Path) -> bool {
    file.file_name().and_then(std::ffi::OsStr::to_str) == Some("README.md")
}

#[cfg(test)]
mod tests {
    use super::is_index;
    use std::path::Path;

    #[test]
    fn index_is_skipped() {
        assert!(is_index(Path::new("docs/adr/README.md")));
        assert!(!is_index(Path::new("docs/adr/0001-x.md")));
    }
}
