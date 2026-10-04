use super::{CallContext, Session, SessionError};
use crate::error::ToolOutcome;
use crate::kernel::Visibility;
use crate::kernel::budget::BudgetCap;
use crate::kernel::event::{CallId, Event};
use crate::kernel::log::read_records;
use crate::kernel::pipeline::{Tool, ToolOutput};
use crate::kernel::state::RefusalReason;
use crate::plan::{Feature, FeatureStatus, Plan, ScopeContract};
use crate::ports::MemFs;
use katu_policy::{
    Enforcement, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId, RuleScope,
    RuleSet, Severity, ToolArgs, ToolName, ToolUse,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Probe {
    calls: AtomicUsize,
}

/// Plano mínimo válido (E06-T06).
fn plan() -> Plan {
    Plan::new(
        ScopeContract::new(
            Vec::new(),
            vec!["**/secrets/**".to_string()],
            Vec::new(),
            "reverter",
        ),
        vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
    )
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
        vocab: 3,
        rules: vec![Rule {
            id: RuleId::from("no-write"),
            statement: "proibido escrever".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            remedy: None,
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
    session.apply(&Event::UserMessage {
        text: "x".into(),
        visibility: Visibility::User,
    })?;

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
    assert!(matches!(result, Err(SessionError::Cost(_))));
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

mod approval;
mod close;
mod context;
mod control;
mod cost;
mod facts;
mod invariants;
mod replay;
mod resume;
mod workspace;
