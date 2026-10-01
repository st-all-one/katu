//! Testes do log append-only (round-trip, sequência, corrupção, replay estável).

use std::path::Path;

use super::{Log, LogErrorKind, read_records, session_path};
use crate::error::ToolOutcome;
use crate::kernel::{CallId, Event, derive_messages, state_of};
use crate::ports::{Fs, MemFs};
use katu_policy::{Phase, ResolvedPath, ToolArgs, ToolName, ToolUse};

#[test]
fn append_then_read_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let mut log = Log::open(&fs, Path::new("/sessions"))?;
    assert_eq!(log.seq(), 0);
    assert_eq!(log.append(&Event::TurnStart { turn: 1 })?, 1);
    assert_eq!(log.append(&Event::TurnEnd { turn: 1 })?, 2);

    let records = read_records(&fs, log.path())?;
    assert_eq!(records.len(), 2);
    assert_eq!(records.first().map(|record| record.seq), Some(1));
    assert_eq!(records.get(1).map(|record| record.seq), Some(2));
    Ok(())
}

#[test]
fn reopen_resumes_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    {
        let mut log = Log::open(&fs, dir)?;
        log.append(&Event::TurnStart { turn: 1 })?;
    }
    let mut reopened = Log::open(&fs, dir)?;
    assert_eq!(reopened.seq(), 1);
    assert_eq!(reopened.append(&Event::TurnEnd { turn: 1 })?, 2);
    Ok(())
}

#[test]
fn sequence_gap_is_detected() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let path = session_path(Path::new("/sessions"));
    let line = b"{\"seq\":1,\"event\":{\"type\":\"turn_start\",\"turn\":1}}\n\
                 {\"seq\":3,\"event\":{\"type\":\"turn_end\",\"turn\":1}}\n";
    fs.write_atomic(&path, line)?;
    let err = read_records(&fs, &path).err();
    assert!(err.is_some_and(|error| error.kind == LogErrorKind::SequenceGap));
    Ok(())
}

#[test]
fn corrupt_line_is_detected() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let path = session_path(Path::new("/sessions"));
    fs.write_atomic(&path, b"{nao e json}\n")?;
    let err = read_records(&fs, &path).err();
    assert!(err.is_some_and(|error| error.kind == LogErrorKind::Corrupt));
    Ok(())
}

#[test]
fn replay_from_log_is_byte_stable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let call = CallId::new("c1");
    let events = vec![
        Event::TurnStart { turn: 7 },
        Event::UserMessage {
            text: "escreve".into(),
        },
        Event::ToolCall {
            call: call.clone(),
            tool: tool()?,
        },
        Event::ToolResult {
            call,
            outcome: ToolOutcome::Ok,
            delta: None,
        },
        Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "teste do log".into(),
        },
        Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        },
        Event::TurnEnd { turn: 7 },
    ];
    {
        let mut log = Log::open(&fs, dir)?;
        for event in &events {
            log.append(event)?;
        }
    }
    let recovered: Vec<Event> = read_records(&fs, &session_path(dir))?
        .into_iter()
        .map(|record| record.event)
        .collect();
    assert_eq!(recovered, events, "o log deve reidratar byte-a-byte");
    assert_eq!(state_of(&recovered)?, state_of(&events)?);
    assert_eq!(derive_messages(&recovered), derive_messages(&events));
    Ok(())
}

fn tool() -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
    Ok(ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: path.clone(),
            bytes: 1,
        },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}
