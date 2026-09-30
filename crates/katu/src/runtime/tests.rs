//! Testes do runtime (E03-T03/T07): recall + escrita pelo gate §42, com memória real in-process.

use std::path::PathBuf;

use katu_core::memory::NoteType;
use katu_core::ports::{FixedClock, Timestamp};

use super::Runtime;
use crate::ports::StdFs;

/// Raiz temporária única por teste.
fn root(label: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("katu-runtime-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[test]
fn runtime_recalls_and_remembers_through_the_gate() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("loop")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "teste")?;
    assert_eq!(runtime.root(), root.as_path());

    let recall = runtime.recall("cache", 5)?;
    assert_eq!(
        recall.report().map(|report| report.kind),
        Some("memory.recall")
    );

    let req = Runtime::note("cache usa LRU", NoteType::Fact, None);
    let write = runtime.remember(&req)?;
    assert_eq!(
        write.report().map(|report| report.kind),
        Some("memory.record")
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn compaction_preview_is_enabled_and_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("compact")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "teste")?;
    runtime.record_user("olá")?;
    runtime.record_assistant("resposta")?;
    let Some(first) = runtime.compaction_preview()? else {
        return Err("compactação devia estar ligada".into());
    };
    let Some(second) = runtime.compaction_preview()? else {
        return Err("compactação devia estar ligada".into());
    };
    assert_eq!(
        first.original_tokens, second.original_tokens,
        "determinístico"
    );
    assert!(first.context.tokens > 0);
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
