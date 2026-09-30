//! `test:<level>` (E13-T01) — camadas de teste explícitas.
//!
//! - `test:unit` — testes de unidade (`--lib`), o que corre rápido no PR;
//! - `test:integration` — alvos de integração (`crates/*/tests/*.rs`, descobertos);
//! - `test:e2e` — o e2e do binário (subprocesso, `-p katu --test cli`);
//! - `test:all` — a suíte completa (`cargo test --workspace`, inclui doc-tests).
//!
//! O CI corre `unit` + lint no PR e `all`/`e2e` no merge (`.github/workflows/`).

use std::fs;
use std::path::Path;
use std::process::Command;

/// Corre a camada pedida.
pub(crate) fn run(level: &str) -> Result<(), String> {
    match level {
        "unit" => run_cargo(&["test", "--workspace", "--lib"]),
        "integration" => {
            let targets = integration_targets(Path::new("crates"))?;
            let mut args: Vec<&str> = vec!["test", "--workspace"];
            for target in &targets {
                args.push("--test");
                args.push(target.as_str());
            }
            run_cargo(&args)
        }
        "e2e" => run_cargo(&["test", "-p", "katu", "--test", "cli"]),
        "all" => run_cargo(&["test", "--workspace"]),
        other => Err(format!(
            "nível de teste desconhecido: {other} (unit|integration|e2e|all)"
        )),
    }
}

/// Alvos de integração em `<root>/*/tests/*.rs` (nome = *stem* do ficheiro).
fn integration_targets(root: &Path) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(root).map_err(|err| format!("lendo {}: {err}", root.display()))?;
    let mut targets: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let dir = entry.path().join("tests");
        if !dir.is_dir() {
            continue;
        }
        let files = fs::read_dir(&dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
        for file in files {
            let file = file.map_err(|err| format!("lendo entrada: {err}"))?;
            let path = file.path();
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
                && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
            {
                targets.push(stem.to_string());
            }
        }
    }
    targets.sort();
    targets.dedup();
    Ok(targets)
}

/// Corre `cargo` com os argumentos dados.
fn run_cargo(args: &[&str]) -> Result<(), String> {
    let status = Command::new("cargo")
        .args(args)
        .status()
        .map_err(|err| format!("cargo {args:?}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {args:?} falhou ({status})"))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::integration_targets;

    #[test]
    fn discovers_integration_targets() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join("katu-test-runner-targets");
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(root.join("katu/tests"))?;
        fs::write(root.join("katu/tests/cli.rs"), "")?;
        fs::write(root.join("katu/tests/notes.txt"), "")?;
        let targets = integration_targets(&root)?;
        assert_eq!(targets, vec!["cli"]);
        Ok(())
    }
}
