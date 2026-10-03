//! Testes do runtime (E03-T03/T07): recall + escrita pelo gate §42, com memória real in-process.

use std::path::PathBuf;

use katu_core::kernel::{CallId, Control, Message};
use katu_core::memory::NoteType;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ModelCapabilities, Thinking};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

use super::{Runtime, RuntimeError};
use katu_core::kernel::SessionError;

mod durability;

mod state;
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
fn resume_reconciles_a_pending_tool_call() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("resume-pending")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    let path = ResolvedPath::from_canonical(runtime.root())?;
    let use_ = ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    };
    runtime.record_user("objetivo")?;
    runtime
        .session_mut()
        .begin_call(CallId::new("pendente"), &use_, 1_000)?;
    drop(runtime); // o processo "morre" a meio de uma tool call

    let resumed = Runtime::resume(&fs, &clock, &root, "cli: resume", None)?;
    resumed.session().verify()?;
    let messages = resumed.messages()?;
    let calls = messages
        .iter()
        .filter(|message| matches!(message, Message::ToolCall { .. }))
        .count();
    let results = messages
        .iter()
        .filter(|message| matches!(message, Message::ToolResult { .. }))
        .count();
    assert_eq!(calls, results, "a retomada não deixa órfãos (L-Q1)");
    assert!(calls >= 1, "a call pendente ficou no histórico emparelhada");
    drop(resumed);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn resume_refuses_a_session_locked_by_another_process() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("resume-locked")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    let lock = runtime
        .session()
        .log_path()
        .parent()
        .ok_or("sessão sem diretório")?
        .join("turn.lock");
    drop(runtime);
    // Simula outro processo vivo: lock fresco com `pid` diferente.
    let other = std::process::id().wrapping_add(1);
    std::fs::write(&lock, format!("{other}\n1000\n"))?;

    let refused = Runtime::resume(&fs, &clock, &root, "cli: resume", None);
    assert!(
        matches!(
            refused,
            Err(RuntimeError::Session(SessionError::TurnLocked { .. }))
        ),
        "um lock fresco de outro processo recusa a retomada: {:?}",
        refused.err().map(|error| error.to_string())
    );
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
