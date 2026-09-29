//! `xtask` — tarefas de verificação do workspace katu.
//!
//! Subcomandos:
//! - `check-layers` — firewall LLM-free (fonte: `layers.toml`);
//! - `check-crate-coverage` — cada crate tem `MODULE.md`;
//! - `check-diag` — logs só estruturados (nenhuma macro de texto livre fora do sink);
//! - `check-docs` — todos os links de `*.md` resolvem.
//!
//! Runbook: `make check`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

/// Firewall de camadas carregado de `layers.toml`.
#[derive(Debug, Deserialize)]
struct Layers {
    layers: BTreeMap<String, Vec<String>>,
}

/// Macros de saída de texto livre proibidas no caminho de produção.
const FORBIDDEN_OUTPUT_MACROS: &[&str] = &["eprintln!", "eprint!", "println!", "print!", "dbg!"];

/// Únicos ficheiros onde escrever texto é legítimo (a borda de diagnóstico).
const DIAG_ALLOWED: &[&str] = &["crates/katu/src/diag.rs"];

/// Documentos de markdown com links a verificar (raiz).
const DOC_ROOTS: &[&str] = &["README.md", "ARCHITECTURE.md", "IMPLEMENTATION_PLAN.md"];

#[allow(clippy::print_stderr, reason = "xtask é a borda de linha de comando")]
fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let result = match task.as_deref() {
        Some("check-layers") => check_layers(),
        Some("check-crate-coverage") => check_crate_coverage(),
        Some("check-diag") => check_diag(),
        Some("check-docs") => check_docs(),
        Some(other) => Err(format!("tarefa desconhecida: {other}")),
        None => {
            Err("uso: xtask <check-layers|check-crate-coverage|check-diag|check-docs>".to_string())
        }
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
        if path.is_dir() && !path.join("MODULE.md").exists() {
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

/// Garante que os logs são **só estruturados** (`katu_core::diag`): nenhuma macro de saída de texto
/// livre em `crates/*/src`, fora do sink de diagnóstico.
fn check_diag() -> Result<(), String> {
    let mut files = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    let mut violations: Vec<String> = Vec::new();
    for file in files {
        let rel = file.to_string_lossy().replace('\\', "/");
        if DIAG_ALLOWED.contains(&rel.as_str()) {
            continue;
        }
        let source =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        for (index, line) in source.lines().enumerate() {
            if let Some(mac) = forbidden_output(line) {
                let line_no = index.saturating_add(1);
                violations.push(format!("{rel}:{line_no}: {mac}"));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-diag falhou (logs estruturados via `katu_core::diag`; só o sink escreve):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Garante que todos os links relativos em `*.md` resolvem para um caminho existente.
fn check_docs() -> Result<(), String> {
    let mut files: Vec<PathBuf> = DOC_ROOTS.iter().map(PathBuf::from).collect();
    collect_by_extension(Path::new("plan"), "md", &mut files)?;
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

/// Recolhe recursivamente ficheiros com a extensão dada sob `dir`.
fn collect_by_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_by_extension(&path, extension, out)?;
        } else if path.extension().and_then(OsStr::to_str) == Some(extension) {
            out.push(path);
        }
    }
    Ok(())
}

/// Devolve a macro de saída proibida encontrada na linha (ignorando comentários).
fn forbidden_output(line: &str) -> Option<&'static str> {
    let code = strip_line_comment(line);
    for mac in FORBIDDEN_OUTPUT_MACROS {
        if code.contains(*mac) {
            return Some(*mac);
        }
    }
    None
}

/// Corta o comentário de fim de linha (sem confundir `https://` com `//`).
fn strip_line_comment(line: &str) -> &str {
    let mut prev_whitespace = true;
    let mut chars = line.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch == '/' && prev_whitespace && matches!(chars.peek(), Some((_, '/'))) {
            return line.get(..index).unwrap_or(line);
        }
        prev_whitespace = ch.is_whitespace();
    }
    line
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
    use super::{depends_on, forbidden_output, markdown_links, strip_line_comment};

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
    fn flags_output_macros() {
        assert_eq!(forbidden_output("    eprintln!(\"x\");"), Some("eprintln!"));
        assert_eq!(forbidden_output("let x = dbg!(1);"), Some("dbg!"));
    }

    #[test]
    fn ignores_comments_and_urls() {
        assert_eq!(forbidden_output("// usa eprintln! para depurar"), None);
        assert_eq!(forbidden_output("let url = \"https://x\";"), None);
        assert_eq!(strip_line_comment("code(); // eprintln!"), "code(); ");
    }

    #[test]
    fn ignores_structured_paths() {
        assert_eq!(
            forbidden_output("katu_core::event!(Level::Info, ev);"),
            None
        );
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
