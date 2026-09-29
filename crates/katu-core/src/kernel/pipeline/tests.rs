use super::{Tool, ToolOutput, dispatch};
use crate::error::ToolOutcome;
use crate::kernel::State;
use katu_policy::{
    Enforcement, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId, RuleScope,
    RuleSet, Severity, ToolArgs, ToolName, ToolUse,
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingTool {
    calls: AtomicUsize,
    outcome: ToolOutcome,
}

impl Tool for CountingTool {
    fn name(&self) -> ToolName {
        ToolName::Write
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ToolOutput::outcome(self.outcome.clone())
    }
}

fn tool() -> CountingTool {
    CountingTool {
        calls: AtomicUsize::new(0),
        outcome: ToolOutcome::Ok,
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

fn deny_secrets() -> Result<RuleSet, PolicyError> {
    let root = ResolvedPath::from_canonical("/work/secrets")?;
    Ok(RuleSet {
        vocab: 2,
        rules: vec![Rule {
            id: RuleId::from("no-secrets"),
            statement: "não escrever em segredos".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            expires_at: None,
            waiver: None,
            examples: RuleExamples {
                negative: vec!["write /work/secrets/token".to_string()],
                positive: Vec::new(),
            },
        }],
    })
}

#[test]
fn deny_does_not_invoke_tool() -> Result<(), PolicyError> {
    let tool = tool();
    let result = dispatch(
        &State::initial(),
        &use_write("/work/secrets/token")?,
        &deny_secrets()?,
        0,
        &tool,
    )?;
    assert!(!result.ran());
    let denied = result.outcome();
    assert!(matches!(denied, ToolOutcome::Denied { .. }));
    if let ToolOutcome::Denied { rule_id, .. } = denied {
        assert_eq!(rule_id.as_str(), "no-secrets");
    }
    assert_eq!(
        tool.calls.load(Ordering::SeqCst),
        0,
        "o executor não pode correr"
    );
    Ok(())
}

#[test]
fn allow_invokes_tool_once() -> Result<(), PolicyError> {
    let tool = tool();
    let rules = RuleSet {
        vocab: 2,
        rules: Vec::new(),
    };
    let result = dispatch(
        &State::initial(),
        &use_write("/work/src/main.rs")?,
        &rules,
        0,
        &tool,
    )?;
    assert!(result.ran());
    assert_eq!(result.outcome(), ToolOutcome::Ok);
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn unknown_vocab_fails_closed_without_effect() -> Result<(), PolicyError> {
    let tool = tool();
    let rules = RuleSet {
        vocab: 99,
        rules: Vec::new(),
    };
    let result = dispatch(
        &State::initial(),
        &use_write("/work/src/main.rs")?,
        &rules,
        0,
        &tool,
    );
    assert!(result.is_err());
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn workspace_capability_unlocks_write_under_the_root() -> Result<(), PolicyError> {
    let workspace = ResolvedPath::from_canonical("/work")?;
    let root = ResolvedPath::from_canonical("/")?;
    let rules = RuleSet {
        vocab: 2,
        rules: vec![Rule {
            id: RuleId::from("only-workspace"),
            statement: "escrever só no workspace".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }],
    };
    let state = State {
        workspace: Some(workspace),
        ..State::initial()
    };
    let inside = dispatch(&state, &use_write("/work/src/main.rs")?, &rules, 0, &tool())?;
    assert!(inside.ran(), "dentro do workspace: a capacidade destranca");
    let outside = dispatch(&state, &use_write("/etc/passwd")?, &rules, 0, &tool())?;
    assert!(!outside.ran(), "fora do workspace: sem capacidade → negado");
    Ok(())
}

/// Política de contenção **real** (E07-T05).
const CONTAINMENT: &str = include_str!("../../../../../policy/containment.toml");

fn use_read(path: &str) -> Result<ToolUse, PolicyError> {
    let resolved = ResolvedPath::from_canonical(path)?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read {
            path: resolved.clone(),
        },
        resolved_paths: vec![resolved.clone()],
        argv: None,
        cwd: resolved,
    })
}

fn workspace_state() -> Result<State, PolicyError> {
    Ok(State {
        workspace: Some(ResolvedPath::from_canonical("/work")?),
        ..State::initial()
    })
}

#[test]
fn sensitive_read_is_denied_even_with_a_workspace() -> Result<(), PolicyError> {
    let rules = RuleSet::from_toml(CONTAINMENT)?;
    let tool = tool();
    let result = dispatch(
        &workspace_state()?,
        &use_read("/work/.env")?,
        &rules,
        0,
        &tool,
    )?;
    assert!(!result.ran(), "o caminho sensível não pode ser lido");
    assert_eq!(
        result.outcome().rule_id().map(RuleId::as_str),
        Some("contain-sensitive-read")
    );
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn normal_read_inside_the_workspace_is_allowed() -> Result<(), PolicyError> {
    let rules = RuleSet::from_toml(CONTAINMENT)?;
    let tool = tool();
    let result = dispatch(
        &workspace_state()?,
        &use_read("/work/src/main.rs")?,
        &rules,
        0,
        &tool,
    )?;
    assert!(result.ran(), "leitura normal dentro do workspace");
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    Ok(())
}
