//! Testes de fecho do turno (L-Q1) e do lock de sessão (L-Q6).

use std::path::Path;

use super::{Session, SessionError};
use crate::kernel::event::{CallId, Event};
use crate::kernel::log::read_records;
use crate::ports::{Fs, MemFs};
use katu_policy::{PolicyError, ResolvedPath, ToolArgs, ToolName, ToolUse};

/// Uso de `read` canónico para os testes.
fn read_use() -> Result<ToolUse, PolicyError> {
    let path = ResolvedPath::from_canonical("/work/a.txt")?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

#[test]
fn reconcile_closes_pending_calls_without_orphans() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.begin_call(CallId::new("c1"), &read_use()?, 0)?;
    assert_eq!(session.state().pending.len(), 1, "há uma call pendente");

    let reconciled = session.reconcile_pending()?;
    assert_eq!(reconciled, 1);
    assert!(session.state().pending.is_empty(), "o pendente foi fechado");
    session.apply(&Event::TurnEnd { turn: 1 })?;
    session.verify()?;

    let events: Vec<Event> = read_records(&fs, session.log_path())?
        .into_iter()
        .map(|record| record.event)
        .collect();
    let calls = events
        .iter()
        .filter(|event| matches!(event, Event::ToolCall { .. }))
        .count();
    let results = events
        .iter()
        .filter(|event| matches!(event, Event::ToolResult { .. }))
        .count();
    assert_eq!(calls, results, "nenhum ToolCall sem ToolResult (I2)");
    assert!(matches!(events.last(), Some(Event::TurnEnd { turn: 1 })));
    Ok(())
}

#[test]
fn reconcile_on_a_clean_turn_adds_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "olá".into()
    })?;
    let before = read_records(&fs, session.log_path())?.len();
    assert_eq!(session.reconcile_pending()?, 0);
    session.apply(&Event::TurnEnd { turn: 1 })?;
    assert_eq!(
        read_records(&fs, session.log_path())?.len(),
        before + 1,
        "só o TurnEnd foi acrescentado"
    );
    Ok(())
}

#[test]
fn a_fresh_lock_from_another_process_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let session = Session::open(&fs, dir)?;
    let other = std::process::id().wrapping_add(1);
    fs.write_atomic(
        &dir.join("turn.lock"),
        format!("{other}\n1000\n").as_bytes(),
    )?;

    let refused = session.acquire_turn_lock(1_001);
    assert!(
        matches!(refused, Err(SessionError::TurnLocked { pid, .. }) if pid == other),
        "um lock fresco de outro processo é recusado: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_stale_lock_is_adopted_and_released() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let session = Session::open(&fs, dir)?;
    let other = std::process::id().wrapping_add(1);
    // `acquired_ms = 0` e `now` muito depois: obsoleto.
    fs.write_atomic(&dir.join("turn.lock"), format!("{other}\n0\n").as_bytes())?;
    session.acquire_turn_lock(1_000_000)?;
    assert!(fs.exists(&dir.join("turn.lock")), "o lock foi reescrito");
    session.release_turn_lock()?;
    assert!(!fs.exists(&dir.join("turn.lock")), "o lock foi libertado");
    // Libertar duas vezes é idempotente (o fecho em erro repete o caminho).
    session.release_turn_lock()?;
    Ok(())
}
