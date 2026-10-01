//! Testes do formato colunar v3 (ADR 0005): secções sem headers, blocos literais, sanitização,
//! projeção guiada pelo registo e aliases de sessão.

use super::{Aliases, Cell, RowTable, Section, Value, emit, project};
mod bench;

use super::colunar::{byte_len, has_control_byte, int_len};
use crate::report::ToolReport;

use proptest::collection;
use proptest::prelude::*;

fn rows<'a>(name: &'a str, cells: Vec<&'a str>) -> Section<'a> {
    let mut table = RowTable::new(name);
    table.push(cells.into_iter().map(Cell::text).collect());
    Section::Rows(table)
}

#[test]
fn emits_section_without_header() {
    assert_eq!(
        emit(&[rows("symbols", vec!["s_1", "main"])]),
        "\u{1e}symbols\ns_1\u{1f}main\n"
    );
}

#[test]
fn omits_empty_sections() {
    assert_eq!(emit(&[Section::Rows(RowTable::new("x"))]), "");
    assert_eq!(
        emit(&[Section::Literal {
            name: "text".into(),
            lines: Vec::new()
        }]),
        ""
    );
    assert_eq!(emit(&[]), "");
}

#[test]
fn booleans_are_zero_or_one() {
    let mut table = RowTable::new("t");
    table.push(vec![Cell::bool(true), Cell::bool(false)]);
    assert_eq!(emit(&[Section::Rows(table)]), "\u{1e}t\n1\u{1f}0\n");
}

#[test]
fn literal_block_keeps_raw_lines() {
    assert_eq!(
        emit(&[Section::literal("text", "fn main() {\n    ok();\n}")]),
        "\u{1d}text\nfn main() {\n    ok();\n}\n"
    );
}

#[test]
fn sanitizes_cells_and_literal_markers() {
    let mut table = RowTable::new("t");
    table.push(vec![Cell::text("a\nb\u{1f}c\u{1e}d")]);
    assert_eq!(emit(&[Section::Rows(table)]), "\u{1e}t\na b c d\n");
    assert_eq!(
        emit(&[Section::Literal {
            name: "t".into(),
            lines: vec!["\u{1d}x".into()]
        }]),
        "\u{1d}t\n x\n"
    );
}

#[test]
fn projects_scalars_as_key_value_rows() {
    let value = Value::map(vec![
        ("path".to_string(), Value::str("src/lib.rs")),
        ("loc".to_string(), Value::int(42)),
    ]);
    assert_eq!(
        emit(&project(&value)),
        "\u{1e}k\npath\u{1f}src/lib.rs\nloc\u{1f}42\n"
    );
}

#[test]
fn projects_list_with_registry_columns() {
    let value = Value::map(vec![(
        "symbols".to_string(),
        Value::list(vec![Value::map(vec![
            ("id".to_string(), Value::str("s_1")),
            ("kind".to_string(), Value::str("fn")),
            ("name".to_string(), Value::str("main")),
            ("start".to_string(), Value::int(3)),
            ("end".to_string(), Value::int(5)),
        ])]),
    )]);
    assert_eq!(
        emit(&project(&value)),
        "\u{1e}symbols\ns_1\u{1f}fn\u{1f}main\u{1f}3\u{1f}5\n"
    );
}

#[test]
fn projects_block_as_literal() {
    let value = Value::map(vec![("text".to_string(), Value::block("a\nb"))]);
    assert_eq!(emit(&project(&value)), "\u{1d}text\na\nb\n");
}

#[test]
fn projects_nested_list_as_child_section() {
    let value = Value::map(vec![(
        "clusters".to_string(),
        Value::list(vec![Value::map(vec![
            ("sym".to_string(), Value::str("s_1")),
            ("name".to_string(), Value::str("m")),
            (
                "hits".to_string(),
                Value::list(vec![Value::map(vec![
                    ("path".to_string(), Value::str("src/a.rs")),
                    ("ln".to_string(), Value::int(3)),
                    ("ty".to_string(), Value::str("code")),
                    ("preview".to_string(), Value::str("let x = 1;")),
                ])]),
            ),
        ])]),
    )]);
    let out = emit(&project(&value));
    assert!(out.contains("\u{1e}clusters\ns_1\u{1f}m\n"), "{out}");
    assert!(
        out.contains(
            "\u{1e}clusters.hits\n0\u{1f}src/a.rs\u{1f}3\u{1f}code\u{1f}\u{1f}let x = 1;\n"
        ),
        "{out}"
    );
}

#[test]
fn aliases_replace_ids_and_paths() {
    let mut aliases = Aliases::new();
    let value = Value::map(vec![
        ("path".to_string(), Value::str("/work/src/lib.rs")),
        ("id".to_string(), Value::str("f_0123456789abcdef")),
    ]);
    let (first, fresh) = aliases.substitute(&value);
    assert!(fresh.is_empty());
    assert!(emit(&project(&first)).contains("/work/src/lib.rs"));
    let (_, fresh) = aliases.substitute(&value);
    assert!(fresh.is_empty());
    let (third, fresh) = aliases.substitute(&value);
    assert_eq!(fresh.len(), 2);
    let rendered = emit(&project(&third));
    assert!(rendered.contains("@1"), "{rendered}");
    assert!(rendered.contains("#1"), "{rendered}");
    // Repetições seguintes não criam aliases novos.
    let (_, again) = aliases.substitute(&value);
    assert!(again.is_empty());
}

/// A varredura SWAR tem de concordar com a ingénua em **todos** os pares de bytes (e em textos
/// longos, com o delimitador em cada posição e fora de UTF-8 ASCII).
#[test]
fn the_swar_scan_never_misses_a_control_byte() {
    for low in 0..=255_u8 {
        for high in 0..=255_u8 {
            let pair = [low, high];
            let text = std::str::from_utf8(&pair).unwrap_or("\u{0}");
            let naive = text.as_bytes().iter().any(|byte| *byte < 0x20);
            assert_eq!(
                has_control_byte(text),
                naive,
                "par {low}/{high} divergiu da varredura ingénua"
            );
        }
    }
    // Delimitador em **cada** fronteira de carácter (inclui as fronteiras de 8 bytes do SWAR).
    let base = "abcçãodefghijklmnopqrstuv";
    for (position, _) in base.char_indices() {
        let text = format!(
            "{}\u{1f}{}",
            base.get(..position).unwrap_or(""),
            base.get(position..).unwrap_or("")
        );
        assert!(has_control_byte(&text), "posição {position} não detetada");
    }
    assert!(
        has_control_byte(&format!("{base}\u{1f}")),
        "fim não detetado"
    );
    assert!(!has_control_byte("sem controlos: ção 漢字 ok"));
}

/// A capacidade reservada é **exata**: a emissão não realoca a meio (o teste falha se o cálculo de
/// `byte_len` divergir do que é escrito).
#[test]
fn the_reserved_capacity_is_exact() {
    let report = ToolReport::new("grep.matches", bench::payload());
    let sections = project(&report.data);
    let reserved = byte_len(&sections);
    let text = emit(&sections);
    assert_eq!(reserved, text.len(), "byte_len divergiu do emitido");
    assert!(text.capacity() >= text.len());
}

/// Inteiros nos extremos (o `int_len` da reserva tem de concordar com o `write!` da emissão).
#[test]
fn the_int_len_matches_what_is_written() {
    for value in [0_i64, -1, 1, 9, 10, -10, i64::MAX, i64::MIN] {
        assert_eq!(int_len(value), value.to_string().len(), "{value}");
    }
}

proptest! {
    #[test]
    fn emission_is_deterministic(values in collection::vec(any::<i64>(), 0..12)) {
        let value = Value::map(vec![(
            "numbers".to_string(),
            Value::list(values.into_iter().map(Value::int).collect()),
        )]);
        let first = emit(&project(&value));
        prop_assert_eq!(&first, &emit(&project(&value)));
    }
}
