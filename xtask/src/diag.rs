//! `check-diag` — logs só estruturados (DF9/E19): nenhuma macro de saída de texto livre em
//! `crates/*/src`, fora do sink de diagnóstico.
//!
//! `diag:coverage` — cobertura de instrumentação por função (E19-T03): fração das funções
//! **instrumentáveis** (não-`const`, fora de testes, com corpo, sem item no topo) que abrem um
//! span (`trace_fn!`/`fn_span!`/`span!`). `katu-policy` é excluída por desenho (firewall: é
//! instrumentada **pelo chamador**). Falha abaixo de 90 %.

#![allow(
    clippy::print_stdout,
    reason = "relatório de cobertura dev-only (a borda é a linha de comando)"
)]

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

/// Palavras que iniciam um **item**; instrumentar antes delas violaria `items_after_statements`.
const ITEM_WORDS: &[&str] = &[
    "const",
    "static",
    "use",
    "type",
    "struct",
    "enum",
    "union",
    "trait",
    "impl",
    "mod",
    "extern",
    "macro_rules",
];

/// Crates medidas (a `katu-policy` fica de fora: firewall — é instrumentada pelo chamador).
const COVERAGE_CRATES: &[&str] = &[
    "katu-core",
    "katu-tools",
    "katu-providers",
    "katu-tui",
    "katu",
];

/// Mede a cobertura de instrumentação por função (E19-T03) e falha abaixo de 90 %.
///
/// # Errors
/// Devolve mensagem se a cobertura das funções instrumentáveis ficar abaixo de 90 %.
pub(crate) fn check_diag_coverage() -> Result<(), String> {
    let mut files = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    let mut total = 0_usize;
    let mut covered = 0_usize;
    let mut consts = 0_usize;
    for file in &files {
        let rel = file.to_string_lossy().replace('\\', "/");
        if !COVERAGE_CRATES
            .iter()
            .any(|c| rel.starts_with(&format!("crates/{c}/")))
        {
            continue;
        }
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("tests") || rel.contains("/tests/") {
            continue;
        }
        // O próprio diag não se instrumenta (recursão no sink): fora da métrica.
        if rel.contains("/diag/") || rel.ends_with("/diag.rs") {
            continue;
        }
        let source =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let (t, c, k) = coverage_in_source(&source);
        total = total.saturating_add(t);
        covered = covered.saturating_add(c);
        consts = consts.saturating_add(k);
    }
    let permille = if total == 0 {
        1000
    } else {
        covered.saturating_mul(1000).saturating_div(total)
    };
    let all = total.saturating_add(consts);
    let permille_all = if all == 0 {
        1000
    } else {
        covered.saturating_mul(1000).saturating_div(all)
    };
    let tenths = permille.saturating_div(10);
    let units = permille.wrapping_rem(10);
    let all_tenths = permille_all.saturating_div(10);
    let all_units = permille_all.wrapping_rem(10);
    println!(
        "diag-coverage: {covered}/{total} instrumentáveis ({tenths}.{units}%); \
         com {consts} const fn: {covered}/{all} ({all_tenths}.{all_units}%)"
    );
    if permille >= 900 {
        Ok(())
    } else {
        Err(format!(
            "diag-coverage falhou: {tenths}.{units}% < 90% ({covered}/{total})"
        ))
    }
}

/// `true` se a linha é a assinatura de uma função (devolve se é `const fn`).
fn is_fn_signature(line: &str) -> Option<bool> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return None;
    }
    let index = trimmed.find("fn ")?;
    let preceded = index == 0
        || trimmed
            .as_bytes()
            .get(index.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace);
    if !preceded {
        return None;
    }
    let after = trimmed.get(index.saturating_add(3)..)?;
    if !after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }
    Some(
        trimmed
            .get(..index)
            .unwrap_or("")
            .split_whitespace()
            .any(|word| word == "const"),
    )
}

/// `true` se o corpo começa por um item (`const`/`use`/…), que não pode seguir um statement.
fn starts_with_item(text: &str) -> bool {
    let mut rest = text.trim_start();
    while let Some(after) = rest.strip_prefix("#[") {
        let Some(end) = after.find(']') else {
            return false;
        };
        rest = after
            .get(end.saturating_add(1)..)
            .unwrap_or("")
            .trim_start();
    }
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || *c == '_')
        .collect();
    !word.is_empty() && ITEM_WORDS.contains(&word.as_str())
}

/// `true` se a função em `index` tem um atributo de teste (`#[test]`/`#[cfg(test)]`).
fn has_test_attr(lines: &[&str], index: usize) -> bool {
    let mut k = index;
    while k > 0 {
        k = k.saturating_sub(1);
        let line = lines.get(k).copied().unwrap_or("").trim();
        if line.starts_with("#[") {
            if line.contains("test") {
                return true;
            }
        } else if line.is_empty() || line.starts_with("//") {
            continue;
        } else {
            break;
        }
    }
    false
}

/// `true` se `index` abre um módulo sob `#[cfg(test)]` (marca o fim do código de produção).
fn is_test_module(lines: &[&str], index: usize) -> bool {
    let line = lines.get(index).copied().unwrap_or("").trim();
    let is_mod = (line.starts_with("mod ")
        || line.starts_with("pub mod ")
        || line.starts_with("pub(crate) mod "))
        && line.contains('{');
    is_mod && has_test_attr(lines, index)
}

/// Avança para depois do bloco entre `{`/`}` que começa em `start`.
fn skip_braced(lines: &[&str], start: usize) -> usize {
    let mut depth = 0_i32;
    let mut index = start;
    while index < lines.len() {
        for ch in lines.get(index).copied().unwrap_or("").chars() {
            match ch {
                '{' => depth = depth.saturating_add(1),
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        index = index.saturating_add(1);
        if depth <= 0 {
            break;
        }
    }
    index
}

/// Conta `(instrumentáveis, cobertas, const)` num ficheiro.
fn coverage_in_source(source: &str) -> (usize, usize, usize) {
    let lines: Vec<&str> = source.lines().collect();
    let mut total = 0_usize;
    let mut covered = 0_usize;
    let mut consts = 0_usize;
    let mut index = 0_usize;
    while index < lines.len() {
        if is_test_module(&lines, index) {
            break;
        }
        let line = lines.get(index).copied().unwrap_or("");
        if line.contains("macro_rules!") {
            index = skip_braced(&lines, index);
            continue;
        }
        if let Some(is_const) = is_fn_signature(line) {
            if has_test_attr(&lines, index) {
                index = index.saturating_add(1);
                continue;
            }
            let mut j = index;
            let mut body: Option<(usize, usize)> = None;
            let mut declaration = false;
            while j < lines.len() && j < index.saturating_add(40) {
                let candidate = lines.get(j).copied().unwrap_or("");
                if let Some(pos) = candidate.find('{') {
                    declaration = candidate.get(..pos).is_some_and(|head| head.contains(';'));
                    body = Some((j, pos));
                    break;
                }
                if candidate.contains(';') {
                    declaration = true;
                    break;
                }
                j = j.saturating_add(1);
            }
            if let Some((body_line, brace)) = body {
                if !declaration {
                    let mut text = String::new();
                    text.push_str(
                        lines
                            .get(body_line)
                            .and_then(|l| l.get(brace.saturating_add(1)..))
                            .unwrap_or(""),
                    );
                    for k in 1..=3 {
                        if let Some(extra) = lines.get(body_line.saturating_add(k)) {
                            text.push(' ');
                            text.push_str(extra);
                        }
                    }
                    let text = text.trim_start();
                    if !starts_with_item(text) {
                        if is_const {
                            consts += 1;
                        } else {
                            total += 1;
                            if text.contains("trace_fn!")
                                || text.contains("fn_span!")
                                || text.contains("span!")
                            {
                                covered += 1;
                            }
                        }
                    }
                }
            }
            index = j.saturating_add(1);
            continue;
        }
        index = index.saturating_add(1);
    }
    (total, covered, consts)
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
    use super::{coverage_in_source, forbidden_output, starts_with_item, strip_line_comment};

    #[test]
    fn coverage_counts_instrumentable_functions() {
        let source = r#"
fn covered() {
    let _span = crate::trace_fn!("m::covered");
}
fn uncovered() {}
const fn konst() {}
#[cfg(test)]
mod tests {
    fn helper() {}
}
"#;
        let (total, covered, consts) = coverage_in_source(source);
        assert_eq!(total, 2, "duas não-const (covered + uncovered)");
        assert_eq!(covered, 1);
        assert_eq!(consts, 1, "const fn excluída");
    }

    #[test]
    fn item_start_is_not_instrumentable() {
        assert!(starts_with_item("const LIMIT: usize = 1;"));
        assert!(starts_with_item("#[cfg(x)] use std::io;"));
        assert!(!starts_with_item("let value = 1;"));
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
