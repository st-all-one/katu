//! Testes do envelope `ToolReport` (DF12/E06-T12, ADR 0005 v3): TOON colunar/JSON coerentes,
//! vazios omitidos, bloco literal e ids content-addressed.

use super::{Cost, Page, ToolReport, content_hash, content_id, fingerprint};
use crate::toon::Value;

fn sample() -> ToolReport {
    ToolReport::new(
        "read.summary",
        Value::map(vec![
            ("path".to_string(), Value::str("src/lib.rs")),
            ("loc".to_string(), Value::int(42)),
        ]),
    )
    .with_id("f_0badc0de")
    .with_hash("0123456789abcdef")
    .with_page(Page::complete(1))
    .with_next(vec!["read f_0badc0de#s_x1".to_string()])
    .with_cost(Cost {
        bytes: 120,
        ms: 1,
        tokens_est: 30,
    })
}

#[test]
fn toon_carries_kind_id_and_data() {
    let toon = sample().to_toon();
    assert!(toon.starts_with("\u{1e}r\n"), "{toon}");
    assert!(
        toon.contains(
            "read.summary\u{1f}f_0badc0de\u{1f}0123456789abcdef\u{1f}\u{1f}1\u{1f}0\u{1f}120\u{1f}1\u{1f}30\n"
        ),
        "{toon}"
    );
    assert!(
        toon.contains("\u{1e}k\npath\u{1f}src/lib.rs\nloc\u{1f}42\n"),
        "{toon}"
    );
    assert!(
        toon.contains("\u{1e}next\nread f_0badc0de#s_x1\n"),
        "{toon}"
    );
}

#[test]
fn json_carries_the_same_fields() -> Result<(), serde_json::Error> {
    let report = sample();
    let json: serde_json::Value = serde_json::from_str(&report.to_json()?)?;
    assert_eq!(
        json.get("kind").and_then(serde_json::Value::as_str),
        Some("read.summary")
    );
    assert_eq!(
        json.get("id").and_then(serde_json::Value::as_str),
        Some("f_0badc0de")
    );
    let data = json.get("data").and_then(serde_json::Value::as_object);
    assert_eq!(
        data.and_then(|map| map.get("loc"))
            .and_then(serde_json::Value::as_i64),
        Some(42)
    );
    Ok(())
}

#[test]
fn omits_absent_blocks_in_both_formats() -> Result<(), serde_json::Error> {
    let report = ToolReport::new("read.full", Value::str("x"));
    let toon = report.to_toon();
    assert!(!toon.contains("\u{1e}next"), "{toon}");
    assert!(!toon.contains('\u{1d}'), "{toon}");
    let json: serde_json::Value = serde_json::from_str(&report.to_json()?)?;
    assert!(json.get("id").is_none());
    assert!(json.get("page").is_none());
    assert!(json.get("next").is_none());
    Ok(())
}

#[test]
fn block_renders_literal_lines() {
    let report = ToolReport::new(
        "read.full",
        Value::map(vec![(
            "text".to_string(),
            Value::block("fn main() {\n    ok();\n}"),
        )]),
    );
    assert!(
        report
            .to_toon()
            .contains("\u{1d}text\nfn main() {\n    ok();\n}\n"),
        "{}",
        report.to_toon()
    );
}

#[test]
fn content_id_is_stable_and_prefixed() {
    let first = content_id("f", b"hello");
    assert_eq!(first, content_id("f", b"hello"));
    assert!(first.starts_with("f_"));
    assert_eq!(first.len(), 18);
    assert_ne!(first, content_id("f", b"world"));
    assert_eq!(content_hash(b"x").len(), 16);
}

#[test]
fn fingerprint_is_deterministic() {
    assert_eq!(fingerprint(b"abc"), fingerprint(b"abc"));
    assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
}

#[test]
fn page_cursor_is_empty_when_none() {
    let page = Page {
        cursor: None,
        total: 5,
        truncated: true,
    };
    let report = ToolReport::new("grep.hits", Value::str("x")).with_page(page);
    assert!(
        report
            .to_toon()
            .contains("grep.hits\u{1f}\u{1f}\u{1f}\u{1f}5\u{1f}1\u{1f}0\u{1f}0\u{1f}0\n")
    );
}
