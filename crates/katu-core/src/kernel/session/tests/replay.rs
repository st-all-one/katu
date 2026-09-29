use super::{Probe, plan, use_write};
use crate::kernel::Message;
use crate::kernel::event::{CallId, Event};
use crate::kernel::session::{CallContext, Session};
use crate::ports::MemFs;
use crate::verify::{CheckStatus, VERIFICATION_SCHEMA_VERSION, VerificationReport};
use katu_policy::{Phase, RuleSet};
use std::path::Path;
use std::sync::atomic::AtomicUsize;

/// Relatório de verificação que passa (E09-T03).
fn verification_report() -> VerificationReport {
    VerificationReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        checks: Vec::new(),
        status: CheckStatus::Pass,
        coverage_bps: 10_000,
        strict: false,
    }
}

#[test]
fn checkpoint_survives_reopen_at_phase_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::Waiver {
        transition: Phase::KnowledgeConsulted,
        reason: "teste de checkpoint".into(),
    })?;
    session.apply(&Event::PhaseTransition {
        to: Phase::KnowledgeConsulted,
        outcome: None,
    })?;
    let written = session.write_checkpoint("objetivo", "planear")?;
    assert_eq!(written.phase, Phase::KnowledgeConsulted);

    let reopened = Session::open(&fs, dir)?;
    assert_eq!(reopened.read_checkpoint()?, Some(written));
    Ok(())
}

#[test]
fn full_loop_verifies_and_messages_come_from_the_log() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "faz isto".into(),
    })?;
    run_allowed_tool(&mut session)?;
    session.apply(&Event::AssistantMessage {
        text: "feito".into(),
    })?;
    session.apply(&Event::Waiver {
        transition: Phase::KnowledgeConsulted,
        reason: "teste do loop completo".into(),
    })?;
    session.apply(&Event::PlanRecorded { plan: plan() })?;
    session.record_verification(&verification_report())?;
    for to in [
        Phase::KnowledgeConsulted,
        Phase::Planned,
        Phase::Implemented,
        Phase::Verified,
        Phase::Persisted,
    ] {
        session.apply(&Event::PhaseTransition { to, outcome: None })?;
    }
    session.apply(&Event::PhaseTransition {
        to: Phase::Closed,
        outcome: Some("feito e verificado".into()),
    })?;
    session.apply(&Event::TurnEnd { turn: 1 })?;

    session.verify()?;
    let messages = session.messages()?;
    assert_eq!(
        messages.len(),
        4,
        "user + ToolCall + ToolResult + assistant"
    );
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    assert!(matches!(messages.get(2), Some(Message::ToolResult { .. })));
    assert_eq!(session.state().phase, Phase::Closed);

    // Reabrir dá o mesmo estado e as mesmas mensagens (o log é a fonte).
    let reopened = Session::open(&fs, dir)?;
    reopened.verify()?;
    assert_eq!(reopened.messages()?, messages);
    Ok(())
}

/// Executa uma tool call permitida no loop de teste (evita ruído no corpo do teste).
fn run_allowed_tool(session: &mut Session<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let allow = RuleSet {
        vocab: 2,
        rules: Vec::new(),
    };
    let outcome = session.tool_call(
        CallId::new("c1"),
        &use_write("/work/src/lib.rs")?,
        CallContext {
            rules: &allow,
            now_millis: 0,
            tool: &probe,
        },
    )?;
    assert!(outcome.ran());
    Ok(())
}

#[test]
fn fork_and_resume_share_the_log_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let src = Path::new("/sessions");
    let dst = Path::new("/forks");
    let mut session = Session::open(&fs, src)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "base".into(),
    })?;

    let mut forked = session.fork(dst)?;
    assert_eq!(forked.state().turn, 1);
    assert_eq!(forked.messages()?.len(), 1);

    session.apply(&Event::TurnEnd { turn: 1 })?;
    forked.apply(&Event::AssistantMessage {
        text: "fork".into(),
    })?;

    assert!(!session.state().turn_open);
    assert!(forked.state().turn_open);
    assert_eq!(forked.messages()?.len(), 2);
    session.verify()?;
    forked.verify()?;
    Ok(())
}
