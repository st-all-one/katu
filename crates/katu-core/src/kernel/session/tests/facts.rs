//! Factos do gate de verificação derivados do log (E09-T03).

use super::{Session, use_write};
use crate::error::ToolOutcome;
use crate::feedback::CommandRecord;
use crate::kernel::event::{CallId, Event};
use crate::ports::MemFs;
use std::path::Path;

fn record() -> CommandRecord {
    CommandRecord {
        id: "cmd-1".to_string(),
        argv: vec!["cargo".to_string(), "test".to_string()],
        cwd: "/sessions".to_string(),
        exit_code: Some(0),
        signal: None,
        timed_out: false,
        duration_ms: 1,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        parent_command_id: None,
    }
}

#[test]
fn changed_files_are_relative_and_keep_only_successful_writes()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;

    session.apply(&Event::ToolCall {
        call: CallId::new("w1"),
        tool: use_write("/sessions/src/main.rs")?,
    })?;
    session.apply(&Event::ToolResult {
        call: CallId::new("w1"),
        outcome: ToolOutcome::Ok,
    })?;
    // Um pedido sem sucesso não alterou o disco: fica fora do diff.
    session.apply(&Event::ToolCall {
        call: CallId::new("w2"),
        tool: use_write("/sessions/secrets/token")?,
    })?;
    session.apply(&Event::ToolResult {
        call: CallId::new("w2"),
        outcome: ToolOutcome::Timeout,
    })?;
    session.apply(&Event::CommandRecorded { record: record() })?;

    assert_eq!(session.changed_files()?, vec!["src/main.rs".to_string()]);
    assert_eq!(session.recorded_commands()?.len(), 1);
    Ok(())
}
