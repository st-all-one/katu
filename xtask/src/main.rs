//! `xtask` — tarefas de verificação do workspace katu.
//!
//! Subcomandos: `check-layers` (firewall de dependências entre crates),
//! `check-crate-coverage` (cada crate tem `MODULE.md`) e `check-diag` (logs só estruturados,
//! via `katu_core::diag`). Runbook: `make check`.

#![forbid(unsafe_code)]

use std::ffi::OsStr;
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
        Some("check-diag") => check_diag(),
        Some(other) => Err(format!("tarefa desconhecida: {other}")),
        None => Err("uso: xtask <check-layers|check-crate-coverage|check-diag>".to_string()),
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

/// Macros de saída de texto livre proibidas no caminho de produção.
const FORBIDDEN_OUTPUT_MACROS: &[&str] = &["eprintln!", "eprint!", "println!", "print!", "dbg!"];

/// Únicos ficheiros onde escrever texto é legítimo (a borda de diagnóstico).
const DIAG_ALLOWED: &[&str] = &["crates/katu/src/diag.rs"];

/// Garante que os logs são **só estruturados** (`katu_core::diag`): nenhuma macro de saída de texto
/// livre em `crates/*/src`, fora do sink de diagnóstico.
///
/// O clippy (`print_stdout`/`print_stderr`/`dbg_macro`) é a primeira linha; este gate é duro contra
/// `#[allow]` e complementa-o.
fn check_diag() -> Result<(), String> {
    let mut files = Vec::new();
    collect_rs(Path::new("crates"), &mut files)?;
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
            "check-diag falhou (logs devem ser estruturados via `katu_core::diag`; só o sink escreve):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Recolhe recursivamente todos os ficheiros `.rs` sob `dir`.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out)?;
        } else if path.extension().and_then(OsStr::to_str) == Some("rs") {
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
    use super::{depends_on, forbidden_output, strip_line_comment};

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
}
