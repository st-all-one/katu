//! Testes do runtime (E03-T03/T07): recall + escrita pelo gate §42, com memória real in-process.

use std::path::PathBuf;

use katu_core::kernel::{Control, Message};
use katu_core::memory::NoteType;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ModelCapabilities, Thinking};

use super::{Runtime, RuntimeError};
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
fn resume_reopens_the_session_and_continues() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("resume")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    let id = runtime.session_id().map(str::to_owned).ok_or("sem id")?;
    runtime.record_user("olá")?;
    runtime.record_assistant("resposta")?;
    let turn = runtime.turn();
    runtime.record_turn_end(turn)?;
    drop(runtime);

    let resumed = Runtime::resume(&fs, &clock, &root, "cli: resume", None)?;
    assert_eq!(resumed.session_id(), Some(id.as_str()));
    assert_eq!(resumed.turn(), turn.saturating_add(1));
    let messages = resumed.messages()?;
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Assistant { text } if text == "resposta")),
        "a retomada vê o histórico durável"
    );
    resumed.session().verify()?;
    drop(resumed);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn resume_closes_an_open_turn_before_the_next() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("resume-open")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    runtime.record_user("olá")?;
    let turn = runtime.turn();
    drop(runtime); // o processo "morre" com o turno aberto

    let resumed = Runtime::resume(&fs, &clock, &root, "cli: resume", None)?;
    assert_eq!(
        resumed.turn(),
        turn.saturating_add(1),
        "fecha o turno aberto e abre o seguinte"
    );
    resumed.session().verify()?;
    drop(resumed);
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

#[test]
fn set_control_is_validated_and_logged() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("control")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "teste")?;

    let unsupported = ModelCapabilities {
        model: "m".to_string(),
        reasoning: false,
    };
    let refused = runtime.set_control(
        &Control::SetThinking {
            thinking: Thinking::Low,
        },
        &unsupported,
    );
    assert!(matches!(refused, Err(RuntimeError::Control(_))));
    assert_eq!(
        runtime.control().thinking,
        Thinking::Off,
        "a recusa não altera o estado nem o log"
    );

    let supported = ModelCapabilities {
        model: "m".to_string(),
        reasoning: true,
    };
    runtime.set_control(
        &Control::SetModel {
            model: "m".to_string(),
        },
        &supported,
    )?;
    runtime.set_control(
        &Control::SetThinking {
            thinking: Thinking::Medium,
        },
        &supported,
    )?;
    assert_eq!(runtime.control().model.as_deref(), Some("m"));
    assert_eq!(runtime.control().thinking, Thinking::Medium);

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn transcript_projects_the_durable_log() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("transcript")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "teste")?;
    runtime.record_user("olá")?;
    runtime.record_assistant("feito")?;
    let lines = runtime.transcript()?;
    assert!(lines.iter().any(|line| line.contains("utilizador")));
    assert!(lines.iter().any(|line| line == "olá"), "{lines:?}");
    assert!(lines.iter().any(|line| line == "feito"), "{lines:?}");
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn checkpoint_records_the_declared_next_action() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("checkpoint")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    let written = runtime.write_checkpoint("verificar")?;
    assert_eq!(written.next_action, "verificar");
    assert_eq!(written.goal, "objetivo");
    let read = runtime
        .checkpoint()?
        .ok_or("checkpoint devia existir depois de escrito")?;
    assert_eq!(read.next_action, "verificar");
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

mod plan;
mod skills;
