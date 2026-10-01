//! `check-diag` — logs só estruturados (DF9/E19): nenhuma macro de saída de texto livre em
//! `crates/*/src`, fora do sink de diagnóstico.
//!
//! A cobertura de instrumentação por função (E19-T03) vive em [`crate::diag_coverage`].

use std::fs;
use std::path::Path;

use crate::walk::collect_by_extension;

/// Macros de saída de texto livre proibidas no caminho de produção.
const FORBIDDEN_OUTPUT_MACROS: &[&str] = &["eprintln!", "eprint!", "println!", "print!", "dbg!"];

/// Únicos ficheiros onde escrever texto é legítimo (a borda de diagnóstico).
const DIAG_ALLOWED: &[&str] = &["crates/katu/src/diag.rs"];

/// Verifica que nenhuma macro de texto livre aparece no código de produção.
pub(crate) fn check_diag() -> Result<(), String> {
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

/// Devolve a macro de saída proibida encontrada na linha (ignorando comentários).
fn forbidden_output(line: &str) -> Option<&'static str> {
    let code = strip_line_comment(line);
    FORBIDDEN_OUTPUT_MACROS
        .iter()
        .find(|mac| code.contains(**mac))
        .copied()
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

#[cfg(test)]
mod tests {
    use super::{forbidden_output, strip_line_comment};

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
