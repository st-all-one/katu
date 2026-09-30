use super::{TrashError, TrashTool, empty, list, restore};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{FixedClock, Fs, MemFs, Timestamp};
use katu_core::report::ToolReport;
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use std::path::{Path, PathBuf};

const ROOT: &str = "/work";
const PATH: &str = "/work/src/a.rs";
const STORED: &str = "/work/.katu/trash/src/a.rs";

fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
    use_path(PATH)
}

fn use_path(path: &str) -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical(path)?;
    let cwd = ResolvedPath::from_canonical(ROOT)?;
    Ok(ToolUse {
        name: ToolName::Trash,
        args: ToolArgs::Trash { path: path.clone() },
        resolved_paths: vec![path],
        argv: None,
        cwd,
    })
}

fn tool<'a>(fs: &'a MemFs, clock: &'a FixedClock) -> TrashTool<'a> {
    TrashTool {
        fs,
        clock,
        root: PathBuf::from(ROOT),
    }
}

fn render(output: &ToolOutput) -> String {
    output
        .report
        .as_ref()
        .map_or_else(String::new, ToolReport::to_toon)
}

#[test]
fn trashes_and_records_index() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"body")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let output = tool(&fs, &clock).execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    assert!(!fs.exists(Path::new(PATH)));
    assert_eq!(fs.read(Path::new(STORED))?, b"body".to_vec());
    let rendered = render(&output);
    assert!(rendered.contains("trash.move\u{1f}"), "{rendered}");
    assert!(rendered.contains("original\u{1f}"), "{rendered}");
    assert!(rendered.contains("\u{1e}refs\n"), "{rendered}");
    let index = fs.read(Path::new("/work/.katu/trash/index.tsv"))?;
    let text = String::from_utf8_lossy(&index);
    assert!(text.contains(STORED), "{text}");
    assert!(text.contains(PATH), "{text}");
    Ok(())
}

#[test]
fn trash_is_reversible() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"body")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    tool(&fs, &clock).execute(&use_()?);
    let restored = restore(&fs, Path::new(ROOT), STORED)?;
    assert_eq!(restored, PathBuf::from(PATH));
    assert!(fs.exists(Path::new(PATH)));
    assert!(!fs.exists(Path::new(STORED)));
    assert_eq!(fs.read(Path::new(PATH))?, b"body".to_vec());
    Ok(())
}

#[test]
fn restore_unknown_is_error() {
    let fs = MemFs::new();
    assert_eq!(
        restore(&fs, Path::new(ROOT), "/work/.katu/trash/nope"),
        Err(TrashError::Unknown)
    );
}

#[test]
fn restore_refuses_occupied_original() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"body")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    tool(&fs, &clock).execute(&use_()?);
    fs.write_atomic(Path::new(PATH), b"new")?;
    assert_eq!(
        restore(&fs, Path::new(ROOT), STORED),
        Err(TrashError::Occupied)
    );
    Ok(())
}

#[test]
fn missing_path_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let output = tool(&fs, &clock).execute(&use_()?);
    assert!(matches!(output.outcome, ToolOutcome::Unavailable { .. }));
    Ok(())
}

#[test]
fn collision_gets_a_suffix() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"one")?;
    fs.write_atomic(Path::new(STORED), b"old")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let output = tool(&fs, &clock).execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    let rendered = render(&output);
    assert!(rendered.contains("a.rs.1000.0"), "{rendered}");
    assert_eq!(fs.read(Path::new(STORED))?, b"old".to_vec());
    Ok(())
}

#[test]
fn list_is_most_recent_first() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"body")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    tool(&fs, &clock).execute(&use_()?);
    let items = list(&fs, Path::new(ROOT));
    assert_eq!(items.len(), 1);
    assert_eq!(items.first().map(|item| item.original.as_str()), Some(PATH));
    assert_eq!(items.first().map(|item| item.at_millis), Some(1_000));
    Ok(())
}

#[test]
fn list_without_index_is_empty() {
    let fs = MemFs::new();
    assert!(list(&fs, Path::new(ROOT)).is_empty());
}

#[test]
fn empty_removes_files_and_clears_the_list() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new(PATH), b"body")?;
    fs.write_atomic(Path::new("/work/src/b.rs"), b"two")?;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let trash = tool(&fs, &clock);
    trash.execute(&use_()?);
    trash.execute(&use_path("/work/src/b.rs")?);
    assert_eq!(list(&fs, Path::new(ROOT)).len(), 2);
    let removed = empty(&fs, Path::new(ROOT))?;
    assert_eq!(removed, 2);
    assert!(!fs.exists(Path::new(STORED)));
    assert!(list(&fs, Path::new(ROOT)).is_empty());
    assert!(
        fs.exists(Path::new("/work/.katu/trash/index.tsv")),
        "o índice fica como rasto de auditoria"
    );
    Ok(())
}

#[test]
fn empty_without_index_is_zero() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    assert_eq!(empty(&fs, Path::new(ROOT))?, 0);
    Ok(())
}
