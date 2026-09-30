//! Matriz de truncagem de `read` (§45.22, E06-T03): limites minúsculos, exatos, chunks únicos
//! enormes, multibyte e orçamento zero — sempre determinística e segura em limites UTF-8.

use super::{ReadBudget, View, views};

/// Renderiza a view `full` de `text` com um orçamento, em TOON.
fn full_toon(text: &str, max_lines: usize, max_bytes: usize) -> String {
    views::build(views::Build {
        view: View::Full,
        path: "p.txt",
        text,
        bytes: text.as_bytes(),
        range: None,
        symbol: None,
        base: None,
        budget: ReadBudget {
            max_lines,
            max_bytes,
        },
    })
    .map_or_else(String::new, |report| report.to_toon())
}

#[test]
fn truncation_of_a_tiny_budget_advances_the_cursor() {
    let tiny = full_toon("a\nb\nc\n", 1, 1_000);
    assert!(
        tiny.contains("\u{1f}2\u{1f}3\u{1f}1\u{1f}0\u{1f}0\u{1f}0\n"),
        "{tiny}"
    );
}

#[test]
fn truncation_of_exact_bytes_keeps_the_line() {
    let exact = full_toon("abc\n", 10, 4);
    assert!(exact.contains("\u{1f}0\u{1f}0\u{1f}0\u{1f}0\n"), "{exact}");
    assert!(exact.contains("abc"), "{exact}");
}

#[test]
fn truncation_of_a_huge_single_chunk_is_utf8_safe() {
    let multibyte = "日".repeat(100);
    let huge = full_toon(&multibyte, 10, 10);
    assert!(huge.contains("\u{1f}1\u{1f}0\u{1f}0\u{1f}0\n"), "{huge}");
    assert!(
        !huge.contains('\u{FFFD}'),
        "cortou a meio de um carácter: {huge}"
    );
    assert_eq!(multibyte.chars().next().map(char::len_utf8), Some(3));
}

#[test]
fn truncation_never_splits_a_multibyte_line() {
    let clipped = full_toon("áéíóú\nüñ\n", 10, 11);
    assert!(clipped.contains("áéíóú"), "{clipped}");
    assert!(!clipped.contains("üñ"), "{clipped}");
    assert!(
        clipped.contains("\u{1f}1\u{1f}0\u{1f}0\u{1f}0\n"),
        "{clipped}"
    );
}

#[test]
fn truncation_with_zero_budget_has_no_cursor() {
    let zero = full_toon("a\n", 10, 0);
    assert!(
        zero.contains("\u{1f}\u{1f}1\u{1f}1\u{1f}0\u{1f}0\u{1f}0\n"),
        "{zero}"
    );
    assert!(zero.contains("path\u{1f}p.txt\n"), "{zero}");
}
