use super::{DEFAULT_LIMIT, SearchTool, search_use};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Fs, FsError, MemFs};
use katu_core::report::ToolReport;
use katu_policy::{ResolvedPath, SearchMode, ToolUse};
use std::path::Path;

const ROOT: &str = "/work";

fn use_at(root: &str, mode: SearchMode, query: &str) -> Result<ToolUse, katu_policy::PolicyError> {
    let root = ResolvedPath::from_canonical(root)?;
    Ok(search_use(&root, query, mode))
}

fn use_(mode: SearchMode, query: &str) -> Result<ToolUse, katu_policy::PolicyError> {
    use_at(ROOT, mode, query)
}

fn seed(fs: &MemFs) -> Result<(), FsError> {
    fs.write_atomic(
        Path::new("/work/src/a.rs"),
        b"pub fn run() {\n    let x = 1; // run here\n}\n",
    )?;
    fs.write_atomic(
        Path::new("/work/src/b.rs"),
        b"use crate::run;\n\n#[test]\nfn t() { run(); }\n",
    )?;
    fs.write_atomic(Path::new("/work/readme.md"), b"# run\n")?;
    Ok(())
}

fn tool(fs: &MemFs, limit: usize) -> SearchTool<'_> {
    SearchTool { fs, limit }
}

fn render(output: &ToolOutput) -> String {
    output
        .report
        .as_ref()
        .map_or_else(String::new, ToolReport::to_toon)
}

#[test]
fn grep_finds_and_clusters() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    seed(&fs)?;
    let output = tool(&fs, DEFAULT_LIMIT).execute(&use_(SearchMode::Grep, "run")?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    let rendered = render(&output);
    assert!(rendered.contains("search.grep\u{1f}"), "{rendered}");
    assert!(rendered.contains("/work/src/a.rs"), "{rendered}");
    assert!(rendered.contains("\u{1e}clusters\n"), "{rendered}");
    assert!(rendered.contains("negative\u{1f}"), "{rendered}");
    Ok(())
}

#[test]
fn grep_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    seed(&fs)?;
    let first = render(&tool(&fs, DEFAULT_LIMIT).execute(&use_(SearchMode::Grep, "run")?));
    let second = render(&tool(&fs, DEFAULT_LIMIT).execute(&use_(SearchMode::Grep, "run")?));
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn grep_applies_the_limit() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    seed(&fs)?;
    let output = tool(&fs, 1).execute(&use_(SearchMode::Grep, "run")?);
    let rendered = render(&output);
    assert!(
        rendered.contains("scanned\u{1f}1\nsearched\u{1f}3\nhits\u{1f}1\n"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn find_ranks_by_name() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    seed(&fs)?;
    let output = tool(&fs, DEFAULT_LIMIT).execute(&use_(SearchMode::Find, "a.rs")?);
    let rendered = render(&output);
    assert!(rendered.contains("search.find\u{1f}"), "{rendered}");
    assert!(rendered.contains("/work/src/a.rs"), "{rendered}");
    Ok(())
}

#[test]
fn ls_maps_semantics() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    seed(&fs)?;
    let output = tool(&fs, DEFAULT_LIMIT).execute(&use_at("/work/src", SearchMode::Ls, "")?);
    let rendered = render(&output);
    assert!(rendered.contains("search.ls\u{1f}"), "{rendered}");
    assert!(rendered.contains("rust"), "{rendered}");
    assert!(rendered.contains("\u{1e}entries\n"), "{rendered}");
    assert!(
        rendered.contains("\u{1f}file\u{1f}rust\u{1f}"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn missing_root_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let output = tool(&fs, DEFAULT_LIMIT).execute(&use_(SearchMode::Grep, "x")?);
    assert!(matches!(output.outcome, ToolOutcome::Unavailable { .. }));
    Ok(())
}
