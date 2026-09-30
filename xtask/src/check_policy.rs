//! `check-policy` (E14-T04) — a separação a três: `policy/` versionado, `.katu/` runtime e
//! segredos fora de ambos.
//!
//! Verifica que `policy/` está sob controlo de versão, que `.katu/` está no `.gitignore` e **não**
//! é versionado, e que nenhum dos dois contém credenciais óbvias.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::walk::collect_by_extension;

/// Prefixos de credencial conhecidos (cinto-e-suspensórios; nunca exaustivo).
const SECRET_PREFIXES: [&str; 7] = [
    "sk-",
    "AKIA",
    "ghp_",
    "gho_",
    "ghs_",
    "xoxb-",
    "-----BEGIN ",
];

/// Extensões de texto onde um segredo apareceria.
const TEXT_EXTENSIONS: [&str; 4] = ["toml", "json", "jsonl", "md"];

/// Ponto de entrada de `check-policy`.
pub(crate) fn check_policy() -> Result<(), String> {
    let mut problems: Vec<String> = Vec::new();
    if git(&["ls-files", "policy"])?.trim().is_empty() {
        problems.push("`policy/` não está sob controlo de versão".to_string());
    }
    if !git_ignored(".katu/audit") {
        problems.push("`.katu/` deve estar no `.gitignore`".to_string());
    }
    if !git(&["ls-files", ".katu"])?.trim().is_empty() {
        problems.push("`.katu/` está versionado (deve ser runtime, gitignored)".to_string());
    }
    scan_secrets(Path::new("policy"), &mut problems)?;
    if Path::new(".katu").is_dir() {
        scan_secrets(Path::new(".katu"), &mut problems)?;
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("check-policy falhou:\n  {}", problems.join("\n  ")))
    }
}

/// `true` se a linha contém um segredo óbvio.
fn looks_secret(line: &str) -> bool {
    SECRET_PREFIXES.iter().any(|prefix| line.contains(prefix))
}

/// Percorre `dir` e reporta linhas com segredos.
fn scan_secrets(dir: &Path, problems: &mut Vec<String>) -> Result<(), String> {
    let mut files: Vec<PathBuf> = Vec::new();
    for extension in TEXT_EXTENSIONS {
        collect_by_extension(dir, extension, &mut files)?;
    }
    files.sort();
    files.dedup();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if looks_secret(line) {
                problems.push(format!(
                    "possível segredo em {}:{}",
                    file.display(),
                    index.saturating_add(1)
                ));
            }
        }
    }
    Ok(())
}

/// Corre um comando `git` e devolve o `stdout`; erro se falhar.
fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("git {args:?}: {err}"))?;
    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|err| format!("git {args:?}: saída não-UTF-8: {err}"))
    } else {
        Err(format!(
            "git {args:?} falhou: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// `true` se `path` é ignorado pelo git.
pub(crate) fn git_ignored(path: &str) -> bool {
    Command::new("git")
        .args(["check-ignore", "-q", path])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::looks_secret;

    #[test]
    fn detects_common_credential_prefixes() {
        assert!(looks_secret("api_key = \"sk-abc123\""));
        assert!(looks_secret("AKIAIOSFODNN7EXAMPLE"));
        assert!(looks_secret("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(!looks_secret("id = \"mem-recall-before-write\""));
        assert!(!looks_secret("token_budget = 100"));
    }
}
