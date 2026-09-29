//! Matriz de truncagem de `read` (§45.22, E06-T03): limites minúsculos, exatos, chunks únicos
//! enormes, multibyte e orçamento zero — sempre determinística e segura em limites UTF-8.

use super::{ReadBudget, View, views};

/// Renderiza a view `full` de `text` com um orçamento, em TOON.
fn full_toon(text: &str, budget: ReadBudget) -> String {
    views::build(views::Build {
        view: View::Full,
        path: "p.txt",
        text,
        bytes: text.as_bytes(),
        range: None,
        symbol: None,
        base: None,
        budget,
    })
    .map_or_else(String::new, |report| report.to_toon())
}

#[test]
fn truncation_matrix_is_deterministic_and_utf8_safe() {
    // 1. Minúsculo: uma linha só.
    let tiny = full_toon(
        "a\nb\nc\n",
        ReadBudget {
            max_lines: 1,
            max_bytes: 1_000,
        },
    );
    assert!(tiny.contains("truncated: true"), "{tiny}");
    assert!(tiny.contains("cursor: 2"), "{tiny}");

    // 2. Bytes exatos: a primeira linha cabe certa.
    let exact = full_toon(
        "abc\n",
        ReadBudget {
            max_lines: 10,
            max_bytes: 4,
        },
    );
    assert!(exact.contains("truncated: false"), "{exact}");
    assert!(exact.contains("abc"), "{exact}");

    // 3. Chunk único enorme: corta no limite do carácter, sem substituir por U+FFFD.
    let multibyte = "日".repeat(100);
    let huge = full_toon(
        &multibyte,
        ReadBudget {
            max_lines: 10,
            max_bytes: 10,
        },
    );
    assert!(huge.contains("truncated: true"), "{huge}");
    assert!(
        !huge.contains('\u{FFFD}'),
        "cortou a meio de um carácter: {huge}"
    );
    assert_eq!(multibyte.chars().next().map(char::len_utf8), Some(3));

    // 4. Multibyte entre linhas: nunca parte um carácter a meio.
    let lines = "áéíóú\nüñ\n";
    let clipped = full_toon(
        lines,
        ReadBudget {
            max_lines: 10,
            max_bytes: 11,
        },
    );
    assert!(clipped.contains("áéíóú"), "{clipped}");
    assert!(!clipped.contains("üñ"), "{clipped}");
    assert!(clipped.contains("truncated: true"), "{clipped}");

    // 5. Orçamento zero: trunca sem cursor (não há paginação que avance).
    let zero = full_toon(
        "a\n",
        ReadBudget {
            max_lines: 10,
            max_bytes: 0,
        },
    );
    assert!(zero.contains("truncated: true"), "{zero}");
    assert!(!zero.contains("cursor:"), "{zero}");
}
