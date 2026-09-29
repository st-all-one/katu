use super::{CallContext, Session, SessionError};
use crate::error::ToolOutcome;
use crate::kernel::Message;
use crate::kernel::budget::BudgetCap;
use crate::kernel::event::{CallId, Event};
use crate::kernel::log::read_records;
use crate::kernel::pipeline::{Tool, ToolOutput};
use crate::kernel::state::RefusalReason;
use crate::ports::MemFs;
use katu_policy::{
    Enforcement, Phase, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId,
    RuleScope, RuleSet, Severity, ToolArgs, ToolName, ToolUse,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Probe {
    calls: AtomicUsize,
}

impl Tool for Probe {
    fn name(&self) -> ToolName {
        ToolName::Write
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ToolOutput::ok()
    }
}

fn use_write(path: &str) -> Result<ToolUse, PolicyError> {
    let resolved = ResolvedPath::from_canonical(path)?;
    Ok(ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: resolved.clone(),
            bytes: 1,
        },
        resolved_paths: vec![resolved.clone()],
        argv: None,
        cwd: resolved,
    })
}

fn rules() -> Result<RuleSet, PolicyError> {
    let root = ResolvedPath::from_canonical("/work")?;
    Ok(RuleSet {
        vocab: 1,
        rules: vec![Rule {
            id: RuleId::from("no-write"),
            statement: "proibido escrever".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            expires_at: None,
            waiver: None,
            examples: RuleExamples {
                negative: vec!["write /work/x".to_string()],
                positive: Vec::new(),
            },
        }],
    })
}

#[test]
fn denied_tool_call_is_logged_but_not_executed() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage { text: "x".into() })?;

    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let result = session.tool_call(
        CallId::new("c1"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules()?,
            now_millis: 0,
            tool: &probe,
        },
    )?;
    assert!(!result.ran());
    assert!(matches!(result.outcome(), ToolOutcome::Denied { .. }));
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    // O pedido e a negação ficam ambos no log.
    assert_eq!(read_records(&fs, session.log_path())?.len(), 4);
    Ok(())
}

#[test]
fn replay_resumes_state_across_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    {
        let mut session = Session::open(&fs, dir)?;
        session.apply(&Event::TurnStart { turn: 3 })?;
    }
    let reopened = Session::open(&fs, dir)?;
    assert_eq!(reopened.state().turn, 3);
    assert!(reopened.state().turn_open);
    Ok(())
}

#[test]
fn refused_event_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    let before = session.state().clone();
    let result = session.apply(&Event::TurnEnd { turn: 1 });
    assert!(matches!(
        result,
        Err(SessionError::Refusal(refusal))
            if matches!(refusal.reason, RefusalReason::NoOpenTurn)
    ));
    assert_eq!(session.state(), &before);
    assert!(read_records(&fs, session.log_path())?.is_empty());
    Ok(())
}

#[test]
fn budget_cap_refuses_tool_call_without_effect() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let cap = BudgetCap {
        tool_calls: Some(0),
        ..BudgetCap::NONE
    };
    let mut session = Session::open_with_cap(&fs, dir, cap)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    let before = session.state().clone();

    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let result = session.tool_call(
        CallId::new("c1"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules()?,
            now_millis: 0,
            tool: &probe,
        },
    );
    assert!(matches!(result, Err(SessionError::Budget(_))));
    assert_eq!(
        probe.calls.load(Ordering::SeqCst),
        0,
        "orçamento recusado = sem efeito"
    );
    assert_eq!(session.state(), &before, "o estado não muda");
    assert_eq!(
        read_records(&fs, session.log_path())?.len(),
        1,
        "só o TurnStart"
    );
    Ok(())
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

    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let allow = RuleSet {
        vocab: 1,
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
    session.apply(&Event::AssistantMessage {
        text: "feito".into(),
    })?;
    session.apply(&Event::Waiver {
        transition: Phase::KnowledgeConsulted,
        reason: "teste do loop completo".into(),
    })?;
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
