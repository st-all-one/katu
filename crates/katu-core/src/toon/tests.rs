//! Testes do emissor TOON (DF12/E06-T12): golden canónico, vazios omitidos, quoting e
//! determinismo.

use super::{Value, emit};
use proptest::collection;
use proptest::prelude::*;

fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::map(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

#[test]
fn emits_canonical_map_with_block_list() {
    let doc = map(vec![
        ("kind", Value::str("read.summary")),
        ("id", Value::str("f_a1b2c3d4")),
        ("loc", Value::int(142)),
        (
            "imports",
            Value::list(vec![Value::str("jwt"), Value::str("db")]),
        ),
        (
            "symbols",
            Value::list(vec![
                map(vec![
                    ("id", Value::str("s_x1")),
                    ("kind", Value::str("fn")),
                    ("range", Value::list(vec![Value::int(12), Value::int(48)])),
                ]),
                map(vec![
                    ("id", Value::str("s_x2")),
                    ("name", Value::str("logout")),
                ]),
            ]),
        ),
    ]);
    let expected = concat!(
        "kind: read.summary\n",
        "id: f_a1b2c3d4\n",
        "loc: 142\n",
        "imports: [jwt, db]\n",
        "symbols:\n",
        "  - id: s_x1\n",
        "    kind: fn\n",
        "    range: [12, 48]\n",
        "  - id: s_x2\n",
        "    name: logout\n",
    );
    assert_eq!(emit(&doc), expected);
}

#[test]
fn omits_empty_fields() {
    let doc = map(vec![
        ("a", Value::int(1)),
        ("empty_list", Value::list(Vec::new())),
        ("empty_map", Value::map(Vec::new())),
        ("b", Value::int(2)),
    ]);
    assert_eq!(emit(&doc), "a: 1\nb: 2\n");
}

#[test]
fn quotes_when_needed_and_escapes() {
    let doc = map(vec![
        ("safe", Value::str("hello world")),
        ("hash", Value::str("read f_a1#s_x1")),
        ("newline", Value::str("a\nb")),
        ("numeric_like", Value::str("12.5")),
        ("empty", Value::str("")),
    ]);
    assert_eq!(
        emit(&doc),
        "safe: hello world\n\
hash: \"read f_a1#s_x1\"\n\
newline: \"a\\nb\"\n\
numeric_like: \"12.5\"\n\
empty: \"\"\n"
    );
}

#[test]
fn float_one_is_emitted_without_trailing_zero() {
    let doc = map(vec![
        ("one", Value::Float(1.0)),
        ("half", Value::Float(0.5)),
    ]);
    assert_eq!(emit(&doc), "one: 1\nhalf: 0.5\n");
}

#[test]
fn nested_map_indents_two_spaces() {
    let doc = map(vec![(
        "cost",
        map(vec![("bytes", Value::int(1240)), ("ms", Value::int(3))]),
    )]);
    assert_eq!(emit(&doc), "cost:\n  bytes: 1240\n  ms: 3\n");
}

#[test]
fn flow_map_is_inline() {
    let inline = Value::list(vec![map(vec![("total", Value::int(1))])]);
    assert_eq!(emit(&inline), "[{total: 1}]\n");
}

#[test]
fn output_never_contains_null() {
    let doc = map(vec![
        ("present", Value::str("x")),
        ("absent", Value::list(Vec::new())),
    ]);
    assert!(!emit(&doc).contains("null"));
}

proptest! {
    #[test]
    fn emit_is_deterministic(values in collection::vec(any::<i64>(), 0..12)) {
        let doc = Value::list(values.into_iter().map(Value::int).collect());
        let first = emit(&doc);
        prop_assert_eq!(&first, &emit(&doc));
    }

    #[test]
    fn emit_of_scalar_map_is_stable(keys in collection::vec("[a-z][a-z0-9_]{0,7}", 0..6)) {
        let doc = Value::map(
            keys.into_iter()
                .map(|key| (key, Value::str("value")))
                .collect(),
        );
        let first = emit(&doc);
        prop_assert_eq!(&first, &emit(&doc));
    }
}
