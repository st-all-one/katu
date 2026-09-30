//! Testes de criação/retoma/ordenação/snapshot (ADR 0008).

use std::path::Path;

use super::Session;
use crate::error::ToolOutcome;
use crate::kernel::Event;
use crate::kernel::event::CallId;
use crate::ports::{Fs, MemFs};
use katu_policy::Phase;

fn repo(fs: &MemFs, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs.write_atomic(&root.join(".git").join("HEAD"), b"ref: refs/heads/main\n")?;
    Ok(())
}

#[test]
fn create_then_resume_restores_state_and_root() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root)?;
    let id = {
        let mut session = Session::create(&fs, root, 1_000, "objetivo")?;
        let id = session.id().cloned().ok_or("sessão sem id")?;
        session.apply(&Event::TurnStart { turn: 1 })?;
        session.apply(&Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "teste".to_string(),
        })?;
        session.apply(&Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        })?;
        id
    };
    let resumed = Session::resume(&fs, root, &id)?;
    assert_eq!(resumed.id(), Some(&id));
    assert_eq!(resumed.root(), root);
    assert_eq!(resumed.state().turn, 1);
    assert!(resumed.state().turn_open);
    assert_eq!(resumed.state().phase, Phase::KnowledgeConsulted);
    Ok(())
}

#[test]
fn snapshot_offset_resumes_the_tail_and_cost() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root)?;
    let id = {
        let mut session = Session::create(&fs, root, 1_000, "objetivo")?;
        let id = session.id().cloned().ok_or("sessão sem id")?;
        session.apply(&Event::TurnStart { turn: 1 })?;
        let call = CallId::new("c1");
        session.apply(&Event::ToolCall {
            call: call.clone(),
            tool: super::use_write("/work/x")?,
        })?;
        session.apply(&Event::ToolResult {
            call,
            outcome: ToolOutcome::Ok,
        })?;
        session.apply(&Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "teste".to_string(),
        })?;
        session.apply(&Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        })?;
        // Cauda depois do snapshot: tem de ser reaplicada a partir do offset.
        session.apply(&Event::UserMessage {
            text: "depois".to_string(),
        })?;
        session.apply(&Event::TurnEnd { turn: 1 })?;
        id
    };
    let resumed = Session::resume(&fs, root, &id)?;
    // Replay total numa cópia sem snapshot, para comparar estado e custo.
    let full = resumed.fork(Path::new("/work/fork"))?;
    assert_eq!(resumed.state(), full.state());
    assert_eq!(
        resumed.cost().global().usage(),
        full.cost().global().usage()
    );
    assert_eq!(resumed.cost().per_tool_used(), full.cost().per_tool_used());
    resumed.verify()?;
    let snapshot = resumed.write_snapshot()?;
    assert!(snapshot.offset > 0, "o snapshot deve fixar o offset do log");
    Ok(())
}

#[test]
fn list_is_temporal() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root)?;
    let late = Session::create(&fs, root, 200, "depois")?;
    let early = Session::create(&fs, root, 100, "antes")?;
    let ids: Vec<_> = Session::list(&fs, root)?
        .into_iter()
        .map(|meta| meta.id)
        .collect();
    assert_eq!(
        ids,
        vec![
            early.id().cloned().ok_or("sem id")?,
            late.id().cloned().ok_or("sem id")?,
        ]
    );
    Ok(())
}
