//! Corpus determinístico do micro-bench (formas reais das tools; chaves alinhadas ao registo v3).

use katu_core::report::{Cost, Page, ToolReport};
use katu_core::toon::Value;

/// Relatórios representativos (espelham as formas reais das tools).
pub(super) fn corpus() -> Vec<ToolReport> {
    vec![summary(), full(), edit(), grep(), recall(), diff(), exec()]
}

fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::map(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn summary() -> ToolReport {
    let symbols: Vec<Value> = (0..12).map(symbol).collect();
    ToolReport::new(
        "read.summary",
        map(vec![
            (
                "path",
                Value::str("crates/katu-core/src/kernel/pipeline/mod.rs"),
            ),
            ("lang", Value::str("rust")),
            ("loc", Value::int(246)),
            (
                "imports",
                Value::list(vec![
                    Value::str("std::path::Path"),
                    Value::str("katu_core::report::ToolReport"),
                ]),
            ),
            ("symbols", Value::list(symbols)),
            (
                "flags",
                Value::list(vec![map(vec![
                    ("ln", Value::int(42)),
                    ("kind", Value::str("TODO")),
                ])]),
            ),
        ]),
    )
    .with_id("f_0123456789abcdef")
    .with_hash("fedcba9876543210")
    .with_page(Page::complete(12))
    .with_cost(Cost {
        bytes: 640,
        ms: 1,
        tokens_est: 160,
    })
    .with_next(vec!["read f_0123456789abcdef#handler_0".to_string()])
}

fn symbol(index: i64) -> Value {
    map(vec![
        ("id", Value::str(format!("s_{index:016x}"))),
        ("kind", Value::str("fn")),
        ("name", Value::str(format!("handler_{index}"))),
        (
            "start",
            Value::int(10_i64.saturating_add(index.saturating_mul(8))),
        ),
        (
            "end",
            Value::int(16_i64.saturating_add(index.saturating_mul(8))),
        ),
    ])
}

fn full() -> ToolReport {
    ToolReport::new(
        "read.full",
        map(vec![
            (
                "path",
                Value::str("crates/katu-core/src/kernel/pipeline/mod.rs"),
            ),
            ("lang", Value::str("rust")),
            ("loc", Value::int(246)),
            ("text", Value::block("pub fn run() {\n    let x = 1;\n}\n")),
        ]),
    )
    .with_id("f_0123456789abcdef")
    .with_hash("fedcba9876543210")
    .with_page(Page {
        cursor: Some(50),
        total: 246,
        truncated: true,
    })
}

fn edit() -> ToolReport {
    ToolReport::new(
        "edit.patch",
        map(vec![
            (
                "path",
                Value::str("crates/katu-core/src/kernel/pipeline/mod.rs"),
            ),
            ("hunks", Value::int(1)),
            ("added", Value::int(1)),
            ("removed", Value::int(1)),
            ("old_hash", Value::str("fedcba9876543210")),
            ("new_hash", Value::str("0011223344556677")),
            ("breaking", Value::bool(false)),
        ]),
    )
    .with_id("f_0123456789abcdef")
    .with_hash("0011223344556677")
}

fn grep() -> ToolReport {
    let hits: Vec<Value> = (0..24).map(hit).collect();
    ToolReport::new(
        "search.grep",
        map(vec![
            ("query", Value::str("authorization")),
            ("root", Value::str("/workspace")),
            ("scanned", Value::int(128)),
            ("searched", Value::int(128)),
            ("hits", Value::int(24)),
            (
                "clusters",
                Value::list(vec![map(vec![
                    ("sym", Value::str("s_0000000000000001")),
                    ("name", Value::str("authorize")),
                    ("hits", Value::list(hits)),
                ])]),
            ),
            ("negative", Value::str("104 ficheiros sem correspondência")),
        ]),
    )
    .with_id("q_0123456789abcdef")
}

fn hit(index: i64) -> Value {
    map(vec![
        (
            "path",
            Value::str(format!("crates/katu-tools/src/tool_{index}.rs")),
        ),
        ("ln", Value::int(12_i64.saturating_add(index))),
        ("ty", Value::str("code")),
        (
            "preview",
            Value::str("let allowed = policy.authorize(&use_)?;"),
        ),
    ])
}

fn recall() -> ToolReport {
    let hits: Vec<Value> = (0..8_i64)
        .map(|index| {
            map(vec![
                ("note", Value::str(format!("n_{index:016x}"))),
                (
                    "statement",
                    Value::str("O gateway limita 100 rps por chave de API"),
                ),
                (
                    "score",
                    Value::int(900_i64.saturating_sub(index.saturating_mul(50))),
                ),
                ("basis", Value::str("measured")),
            ])
        })
        .collect();
    ToolReport::new(
        "memory.recall",
        map(vec![
            ("query", Value::str("rate limit gateway")),
            ("hits", Value::list(hits)),
        ]),
    )
    .with_id("recall:8")
    .with_page(Page::complete(8))
}

fn diff() -> ToolReport {
    let hunks: Vec<Value> = (0..4).map(hunk).collect();
    ToolReport::new(
        "read.diff",
        map(vec![
            (
                "path",
                Value::str("crates/katu-core/src/kernel/pipeline/mod.rs"),
            ),
            ("added", Value::int(4)),
            ("removed", Value::int(4)),
            ("hunks", Value::list(hunks)),
        ]),
    )
    .with_id("f_0123456789abcdef")
    .with_hash("fedcba9876543210")
}

fn hunk(index: i64) -> Value {
    map(vec![
        (
            "old_start",
            Value::int(10_i64.saturating_add(index.saturating_mul(20))),
        ),
        ("old_len", Value::int(6)),
        (
            "new_start",
            Value::int(10_i64.saturating_add(index.saturating_mul(20))),
        ),
        ("new_len", Value::int(7)),
        (
            "lines",
            Value::list(vec![
                Value::str(" pub fn run() {"),
                Value::str("-    let x = 1;"),
                Value::str("+    let x = 2;"),
                Value::str(" }"),
            ]),
        ),
    ])
}

fn exec() -> ToolReport {
    ToolReport::new(
        "exec.run",
        map(vec![
            (
                "argv",
                Value::list(vec![
                    Value::str("cargo"),
                    Value::str("test"),
                    Value::str("--workspace"),
                ]),
            ),
            ("cwd", Value::str("/workspace")),
            ("exit", Value::int(0)),
            ("signal", Value::int(0)),
            ("timed_out", Value::bool(false)),
            ("duration_ms", Value::int(4213)),
            (
                "stdout",
                Value::block("running 367 tests\n...\ntest result: ok. 367 passed\n"),
            ),
            ("stderr", Value::block("")),
        ]),
    )
    .with_id("x_0123456789abcdef")
}
