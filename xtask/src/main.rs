//! `xtask` — tarefas de verificação do workspace katu.
//!
//! Subcomandos:
//! - `check-layers` — firewall LLM-free (fonte: `layers.toml`);
//! - `check-crate-coverage` — cada crate tem `MODULE.md`;
//! - `check-diag` — logs só estruturados (nenhuma macro de texto livre fora do sink);
//! - `check-schemas` — schema das tools válido (E06-T02);
//! - `check-docs` — todos os links de `*.md` resolvem;
//! - `gate:bench` — nenhum número publicado sem base e artefacto (DF5/E15-T02);
//! - `policy:audit` — regras `Enforced`/`Advisory` coerentes (E02-T04);
//! - `ledger:validate` — ledger de cobertura consistente com `policy/` (E02-T06).
//!
//! Runbook: `make check`.

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "binário: sem API externa; os módulos internos usam pub(crate)"
)]

mod bench;
mod diag;
mod docs;
mod ledger;
mod policy;
mod schemas;
#[cfg(feature = "tokenizer")]
mod toon_bench;
mod walk;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use diag::check_diag;
use walk::collect_by_extension;

/// Firewall de camadas carregado de `layers.toml`.
#[derive(Debug, Deserialize)]
struct Layers {
    layers: BTreeMap<String, Vec<String>>,
}

/// Documentos de markdown com links a verificar (raiz).
const DOC_ROOTS: &[&str] = &["README.md", "ARCHITECTURE.md", "IMPLEMENTATION_PLAN.md"];

#[allow(clippy::print_stderr, reason = "xtask é a borda de linha de comando")]
fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();
    let result = match task.as_deref() {
        Some("check-layers") => check_layers(),
        Some("check-crate-coverage") => check_crate_coverage(),
        Some("check-diag") => check_diag(),
        Some("check-schemas") => schemas::check_schemas(),
        Some("check-docs") => check_docs(),
        Some("gate:bench") => bench::gate_bench(&rest),
        Some("policy:audit") => policy::policy_audit(&rest),
        Some("ledger:validate") => ledger::ledger_validate(&rest),
        #[cfg(feature = "tokenizer")]
        Some("bench-toon") => toon_bench::run(),
        Some(other) => Err(format!("tarefa desconhecida: {other}")),
        None => Err(
            "uso: xtask <check-layers|check-crate-coverage|check-diag|check-schemas|check-docs|gate:bench|policy:audit|ledger:validate>"
                .to_string(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Verifica o firewall LLM-free, lendo as arestas proibidas de `layers.toml`.
fn check_layers() -> Result<(), String> {
    let text =
        fs::read_to_string("layers.toml").map_err(|err| format!("lendo layers.toml: {err}"))?;
    let parsed: Layers =
        toml::from_str(&text).map_err(|err| format!("layers.toml inválido: {err}"))?;
    let mut violations: Vec<String> = Vec::new();
    for (krate, forbidden) in &parsed.layers {
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
        if path.is_dir() && !walk::is_foreign_root(&path) && !path.join("MODULE.md").exists() {
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or("?");
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

/// Garante que todos os links relativos em `*.md` resolvem para um caminho existente e que cada
/// ADR tem `## Alternatives considered` (E14-T01).
fn check_docs() -> Result<(), String> {
    docs::check_adrs()?;
    let mut files: Vec<PathBuf> = DOC_ROOTS.iter().map(PathBuf::from).collect();
    collect_by_extension(Path::new("plan"), "md", &mut files)?;
    collect_by_extension(Path::new("docs"), "md", &mut files)?;
    let mut violations: Vec<String> = Vec::new();
    for file in files {
        let source =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let base = file.parent().unwrap_or_else(|| Path::new("."));
        for link in markdown_links(&source) {
            let target = link.split('#').next().unwrap_or("");
            if is_external(target) {
                continue;
            }
            if !base.join(target).exists() {
                violations.push(format!("{}: link quebrado -> {link}", file.display()));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("check-docs falhou:\n  {}", violations.join("\n  ")))
    }
}

/// `true` para links que não apontam para um ficheiro do repositório.
fn is_external(link: &str) -> bool {
    link.is_empty()
        || link.starts_with('#')
        || link.starts_with("http://")
        || link.starts_with("https://")
        || link.starts_with("mailto:")
}

/// Extrai os destinos de links markdown `](destino)`.
fn markdown_links(source: &str) -> Vec<String> {
    source
        .match_indices("](")
        .filter_map(|(pos, marker)| {
            let rest = source.get(pos.saturating_add(marker.len())..)?;
            let end = rest.find(')')?;
            rest.get(..end).map(str::to_string)
        })
        .collect()
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
    use super::{depends_on, markdown_links};

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

    #[test]
    fn extracts_markdown_links() {
        let links = markdown_links("ver [a](plan/01.md) e [b](../x/y.md#z).");
        assert_eq!(
            links,
            vec!["plan/01.md".to_string(), "../x/y.md#z".to_string()]
        );
    }
}
