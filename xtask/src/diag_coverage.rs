//! `diag:coverage` — cobertura de instrumentação por função (E19-T03).
//!
//! Mede a fração das funções **instrumentáveis** (não-`const`, fora de testes, com corpo, sem item
//! no topo) que abrem um span (`trace_fn!`/`fn_span!`/`span!`). `katu-policy` fica fora por desenho
//! (firewall: é instrumentada **pelo chamador**), tal como `tests/`, `examples/` e `benches/` — não
//! são código de produção.
//!
//! **Duas medidas, ambas travadas em 90 %:**
//!
//! - **instrumentáveis** — o alvo do trabalho (instrumentar o que se pode instrumentar);
//! - **todas as funções** — a mesma conta com as `const fn` no denominador. Uma `const fn` não pode
//!   abrir um span num build com a `feature = "instrument"` (o `Span::start_function` não é
//!   `const`), pelo que a única forma de a contar é **deixar de ser `const`** (quando não é usada em
//!   contexto `const`) ou não a ter.
//!
//! Sem a segunda trava, o número com `const fn` deslizava para baixo de 90 % a cada `const fn` nova
//! (Q-15/P-01 acrescentaram várias) sem que nada falhasse.

#![allow(
    clippy::print_stdout,
    reason = "relatório de cobertura dev-only (a borda é a linha de comando)"
)]

use std::fs;
use std::path::Path;

use crate::walk::collect_by_extension;

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
#[allow(
    clippy::arithmetic_side_effects,
    reason = "percentagem dev-only: o divisor > 0 é verificado antes de dividir"
)]
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
        if !is_production(&rel, name) {
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
    if permille < 900 {
        return Err(format!(
            "diag-coverage falhou: {tenths}.{units}% das funções instrumentáveis < 90% \
             ({covered}/{total}) — abra um span (`trace_fn!`) nas que faltam"
        ));
    }
    if permille_all < 900 {
        return Err(format!(
            "diag-coverage falhou: {all_tenths}.{all_units}% de todas as funções < 90% \
             ({covered}/{all}, com {consts} `const fn`) — instrumente as que faltam ou tire o \
             `const` às que não são usadas em contexto `const`"
        ));
    }
    Ok(())
}

/// `true` se o caminho é **código de produção** (fora de testes, exemplos, *benches* e do diag).
///
/// Exemplos e *benches* não são instrumentados nem contados: não correm no produto. O próprio `diag`
/// não se instrumenta (recursão no sink).
#[must_use]
fn is_production(rel: &str, name: &str) -> bool {
    if name.contains("tests") || rel.contains("/tests/") {
        return false;
    }
    if rel.contains("/examples/") || rel.contains("/benches/") {
        return false;
    }
    !(rel.contains("/diag/") || rel.ends_with("/diag.rs"))
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
        if line.starts_with("#[") && line.contains("test") {
            return true;
        }
        if !line.starts_with("#[") && !line.is_empty() && !line.starts_with("//") {
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

/// Cabeça do corpo: do `{` até 3 linhas depois (onde vive o primeiro statement).
fn body_head(lines: &[&str], body_line: usize, brace: usize) -> String {
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
    text.trim_start().to_string()
}

/// `true` se a cabeça do corpo já abre um span (`trace_fn!`/`fn_span!`/`span!`).
fn head_is_instrumented(head: &str) -> bool {
    head.contains("trace_fn!") || head.contains("fn_span!") || head.contains("span!")
}

/// Conta `(instrumentáveis, cobertas, const)` num ficheiro.
fn coverage_in_source(source: &str) -> (usize, usize, usize) {
    let lines: Vec<&str> = source.lines().collect();
    let mut counts = (0_usize, 0_usize, 0_usize);
    let mut index = 0_usize;
    while index < lines.len() {
        if is_test_module(&lines, index) {
            break;
        }
        let line = lines.get(index).copied().unwrap_or("");
        if line.contains("macro_rules!") {
            index = skip_braced(&lines, index);
        } else if is_fn_signature(line).is_some() {
            let (next, delta) = account_fn(&lines, index);
            counts.0 = counts.0.saturating_add(delta.0);
            counts.1 = counts.1.saturating_add(delta.1);
            counts.2 = counts.2.saturating_add(delta.2);
            index = next;
        } else {
            index = index.saturating_add(1);
        }
    }
    counts
}

/// Classifica uma função: `(próximo índice, incremento (instrumentáveis, cobertas, const))`.
fn account_fn(lines: &[&str], index: usize) -> (usize, (usize, usize, usize)) {
    let zero = (0_usize, 0_usize, 0_usize);
    let is_const = lines
        .get(index)
        .copied()
        .and_then(is_fn_signature)
        .unwrap_or(false);
    if has_test_attr(lines, index) {
        return (index.saturating_add(1), zero);
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
    let next = j.saturating_add(1);
    if declaration {
        return (next, zero);
    }
    let Some((body_line, brace)) = body else {
        return (next, zero);
    };
    let head = body_head(lines, body_line, brace);
    if starts_with_item(&head) {
        return (next, zero);
    }
    if is_const {
        return (next, (0, 0, 1));
    }
    if head_is_instrumented(&head) {
        (next, (1, 1, 0))
    } else {
        (next, (1, 0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::{coverage_in_source, is_production, starts_with_item};

    #[test]
    fn only_production_paths_are_measured() {
        assert!(is_production(
            "crates/katu-core/src/kernel/log.rs",
            "log.rs"
        ));
        assert!(!is_production(
            "crates/katu-core/src/kernel/log/tests.rs",
            "tests.rs"
        ));
        assert!(!is_production(
            "crates/katu/examples/measure_mvk.rs",
            "measure_mvk.rs"
        ));
        assert!(!is_production(
            "crates/katu/benches/throughput.rs",
            "throughput.rs"
        ));
        assert!(!is_production("crates/katu-core/src/diag/mod.rs", "mod.rs"));
        assert!(!is_production("crates/katu-core/src/diag.rs", "diag.rs"));
    }

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
}
