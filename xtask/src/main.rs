//! `xtask` — tarefas de verificação do workspace katu.
//!
//! Subcomandos: `check-layers` (firewall de dependências entre crates) e `check-crate-coverage`
//! (cada crate tem `MODULE.md`). Runbook: `make check`.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Arestas proibidas: `(crate, dependências que ele NÃO pode ter)`.
const FORBIDDEN_EDGES: &[(&str, &[&str])] = &[
    (
        "katu-core",
        &["katu-tools", "katu-providers", "katu-tui", "knudge-core"],
    ),
    (
        "katu-policy",
        &[
            "katu-core",
            "katu-tools",
            "katu-providers",
            "katu-tui",
            "knudge-core",
        ],
    ),
    ("katu-tools", &["katu-providers", "katu-tui", "knudge-core"]),
];

#[allow(clippy::print_stderr, reason = "xtask é a borda de linha de comando")]
fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let result = match task.as_deref() {
        Some("check-layers") => check_layers(),
        Some("check-crate-coverage") => check_crate_coverage(),
        Some(other) => Err(format!("tarefa desconhecida: {other}")),
        None => Err("uso: xtask <check-layers|check-crate-coverage>".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Verifica o firewall LLM-free: crates puros não dependem de providers/adaptadores.
fn check_layers() -> Result<(), String> {
    let mut violations: Vec<String> = Vec::new();
    for &(krate, forbidden) in FORBIDDEN_EDGES {
        let manifest = read_manifest(krate)?;
        for dep in forbidden {
            if depends_on(&manifest, dep) {
                violations.push(format!("{krate} não pode depender de {dep}"));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-layers falhou:\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Garante que cada crate em `crates/` tem um `MODULE.md`.
fn check_crate_coverage() -> Result<(), String> {
    let entries = fs::read_dir("crates").map_err(|err| format!("lendo crates/: {err}"))?;
    let mut missing: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !path.join("MODULE.md").exists() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            missing.push(name.to_string());
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-crate-coverage falhou (sem MODULE.md): {}",
            missing.join(", ")
        ))
    }
}

/// Lê o `Cargo.toml` de um crate do workspace.
fn read_manifest(krate: &str) -> Result<String, String> {
    let path = manifest_path(krate);
    fs::read_to_string(&path).map_err(|err| format!("lendo {}: {err}", path.display()))
}

/// Caminho do `Cargo.toml` de um crate.
fn manifest_path(krate: &str) -> PathBuf {
    Path::new("crates").join(krate).join("Cargo.toml")
}

/// `true` se o manifesto declara uma dependência chamada `dep`.
fn depends_on(manifest: &str, dep: &str) -> bool {
    manifest.lines().any(|line| {
        line.trim_start()
            .strip_prefix(dep)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

#[cfg(test)]
mod tests {
    use super::depends_on;

    #[test]
    fn detects_dependency_edges() {
        let manifest = "[dependencies]\nkatu-core = { path = \"../katu-core\" }\n";
        assert!(depends_on(manifest, "katu-core"));
    }

    #[test]
    fn ignores_unrelated_names() {
        let manifest = "[package]\nname = \"katu-core\"\n";
        assert!(!depends_on(manifest, "katu-tools"));
    }
}
