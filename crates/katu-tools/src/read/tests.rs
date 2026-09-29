//! Testes das views de `read` (E06-T03).

use super::{LineRange, ReadBudget, ReadTool, View, views};
use katu_core::error::ToolOutcome;
use katu_core::kernel::Tool;
use katu_core::ports::{Fs, MemFs};
use katu_core::toon::Value;
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

const SOURCE: &str = "\
use std::fmt;

pub struct Point {
    x: i32,
}

pub fn new(x: i32) -> Self {
    // TODO: validar
    Self { x }
}
";

fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical("/work/src/point.rs")?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

fn report(view: View, range: Option<LineRange>, symbol: Option<&str>) -> String {
    views::build(views::Build {
        view,
        path: "src/point.rs",
        text: SOURCE,
        bytes: SOURCE.as_bytes(),
        range,
        symbol,
        budget: ReadBudget::default(),
    })
    .map_or_else(String::new, |report| report.to_toon())
}

#[test]
fn summary_carries_symbols_imports_and_next() {
    let toon = report(View::Summary, None, None);
    assert!(toon.starts_with("kind: read.summary\n"), "{toon}");
    assert!(toon.contains("lang: rust\n"), "{toon}");
    assert!(toon.contains("imports: [use std::fmt]\n"), "{toon}");
    assert!(toon.contains("name: Point\n"), "{toon}");
    assert!(toon.contains("name: new\n"), "{toon}");
    assert!(toon.contains("next:"), "{toon}");
}

#[test]
fn full_returns_a_literal_block_with_ids() {
    let toon = report(View::Full, None, None);
    assert!(toon.contains("kind: read.full\n"), "{toon}");
    assert!(toon.contains("id: f_"), "{toon}");
    assert!(toon.contains("hash: "), "{toon}");
    assert!(toon.contains("text: |\n"), "{toon}");
    assert!(toon.contains("  use std::fmt;\n"), "{toon}");
}

#[test]
fn full_round_trips_the_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = views::build(views::Build {
        view: View::Full,
        path: "src/point.rs",
        text: SOURCE,
        bytes: SOURCE.as_bytes(),
        range: None,
        symbol: None,
        budget: ReadBudget::default(),
    })
    .ok_or("sem relatório")?;
    let Value::Map(entries) = &report.data else {
        return Err("data não é mapa".into());
    };
    let text = entries
        .iter()
        .find(|(key, _)| key == "text")
        .map(|(_, value)| value);
    assert_eq!(text, Some(&Value::Block(SOURCE.to_string())));
    Ok(())
}

#[test]
fn outline_lists_symbols_with_ranges() {
    let toon = report(View::Outline, None, None);
    assert!(toon.contains("kind: read.outline\n"), "{toon}");
    assert!(toon.contains("kind: struct\n"), "{toon}");
    assert!(toon.contains("kind: fn\n"), "{toon}");
    assert!(toon.contains("range: [3, 5]\n"), "{toon}");
}

#[test]
fn symbol_returns_only_the_body() {
    let toon = report(View::Symbol, None, Some("new"));
    assert!(toon.contains("kind: read.symbol\n"), "{toon}");
    assert!(toon.contains("symbol: new\n"), "{toon}");
    assert!(toon.contains("Self { x }\n"), "{toon}");
    assert!(
        !toon.contains("use std::fmt"),
        "não devia incluir imports: {toon}"
    );
}

#[test]
fn unknown_symbol_is_explicit() {
    let toon = report(View::Symbol, None, Some("nope"));
    assert!(toon.contains("found: false\n"), "{toon}");
    assert!(toon.contains("symbols:"), "{toon}");
}

#[test]
fn range_returns_the_requested_slice() {
    let toon = report(View::Range, Some(LineRange { start: 3, end: 5 }), None);
    assert!(toon.contains("range: [3, 5]\n"), "{toon}");
    assert!(toon.contains("pub struct Point {\n"), "{toon}");
    assert!(!toon.contains("use std::fmt"), "{toon}");
}

#[test]
fn small_budget_truncates_deterministically() {
    let budget = ReadBudget {
        max_lines: 2,
        max_bytes: 1_000,
    };
    let report = views::build(views::Build {
        view: View::Full,
        path: "p.rs",
        text: SOURCE,
        bytes: SOURCE.as_bytes(),
        range: None,
        symbol: None,
        budget,
    });
    let toon = report.map_or_else(String::new, |report| report.to_toon());
    assert!(toon.contains("truncated: true"), "{toon}");
    assert!(toon.contains("cursor: 3"), "{toon}");
}

#[test]
fn diff_is_unavailable() {
    assert!(report(View::Diff, None, None).is_empty());
    assert_eq!(View::parse("summary"), Some(View::Summary));
    assert_eq!(View::parse("bogus"), None);
}

#[test]
fn tool_reads_and_reports() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(
        std::path::Path::new("/work/src/point.rs"),
        SOURCE.as_bytes(),
    )?;
    let tool = ReadTool {
        fs: &fs,
        view: View::Summary,
        range: None,
        symbol: None,
        budget: ReadBudget::default(),
    };
    let output = tool.execute(&use_()?);
    let report = output.report.as_ref().ok_or("sem relatório")?;
    assert!(report.to_toon().contains("kind: read.summary"));
    Ok(())
}

#[test]
fn missing_file_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let tool = ReadTool {
        fs: &fs,
        view: View::Summary,
        range: None,
        symbol: None,
        budget: ReadBudget::default(),
    };
    assert!(matches!(
        tool.execute(&use_()?).outcome,
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}
