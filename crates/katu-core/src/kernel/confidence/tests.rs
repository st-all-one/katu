//! Testes da extração de observações (Q-11): recusa honrada, violação, aprovação e por tool.

use katu_policy::{
    Enforcement, Evidence, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId,
    RuleScope, RuleSet, Severity, Threshold, ToolArgs, ToolName, ToolUse,
};

use super::{enforced_verdicts, rule_trials, tool_trials};
use crate::error::ToolOutcome;
use crate::kernel::event::{CallId, Event};

mod bench;

fn read_use() -> Result<ToolUse, PolicyError> {
    let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

fn denied(call: &str, rule: &str) -> Event {
    let rule_id = RuleId::from(rule);
    Event::ToolResult {
        call: CallId::new(call),
        outcome: ToolOutcome::Denied {
            evidence: Evidence::new("facto", "argumento", rule_id.clone()),
            rule_id,
        },
        delta: None,
    }
}

fn ran(call: &str) -> Event {
    Event::ToolResult {
        call: CallId::new(call),
        outcome: ToolOutcome::Ok,
        delta: None,
    }
}

fn call(call_id: &str, name: ToolName) -> Result<Event, PolicyError> {
    let mut use_ = read_use()?;
    use_.name = name;
    Ok(Event::ToolCall {
        call: CallId::new(call_id),
        tool: use_,
    })
}

#[test]
fn a_denied_call_that_did_not_run_is_honored() -> Result<(), PolicyError> {
    let events = vec![call("c1", ToolName::Read)?, denied("c1", "r1")];
    let trials = rule_trials(&events);
    let observed = trials.get(&RuleId::from("r1")).copied().unwrap_or_default();
    assert_eq!(observed.trials(), 1);
    assert_eq!(observed.successes(), 1);
    Ok(())
}

#[test]
fn a_denied_call_that_ran_is_a_violation() -> Result<(), PolicyError> {
    let events = vec![call("c1", ToolName::Read)?, denied("c1", "r1"), ran("c1")];
    let trials = rule_trials(&events);
    let observed = trials.get(&RuleId::from("r1")).copied().unwrap_or_default();
    assert_eq!(observed.trials(), 1);
    assert_eq!(observed.successes(), 0);
    Ok(())
}

#[test]
fn the_approved_retry_does_not_collide_with_the_denial() -> Result<(), PolicyError> {
    let events = vec![
        call("c1", ToolName::Read)?,
        denied("c1", "r1"),
        Event::ApprovalGranted {
            rule_id: RuleId::from("r1"),
            capability: katu_policy::Capability::ReadPath {
                root: ResolvedPath::from_canonical("/work")?,
            },
            reason: "justificado".into(),
            granted_by: "humano".into(),
            signature: "test-sig".into(),
        },
        call("c1#approved", ToolName::Read)?,
        ran("c1#approved"),
    ];
    let trials = rule_trials(&events);
    let observed = trials.get(&RuleId::from("r1")).copied().unwrap_or_default();
    assert_eq!(observed.trials(), 2, "recusa + aprovação");
    assert_eq!(observed.successes(), 2, "a re-execução tem id próprio");
    Ok(())
}

#[test]
fn unavailable_without_a_rule_is_not_a_trial() -> Result<(), PolicyError> {
    let events = vec![
        call("c1", ToolName::Read)?,
        Event::ToolResult {
            call: CallId::new("c1"),
            outcome: ToolOutcome::Unavailable {
                control: katu_policy::ControlId::new("approval"),
                rule_id: None,
            },
            delta: None,
        },
    ];
    assert!(rule_trials(&events).is_empty());
    Ok(())
}

#[test]
fn the_tool_contract_counts_timeouts_against_the_tool() -> Result<(), PolicyError> {
    let events = vec![
        call("c1", ToolName::Exec)?,
        ran("c1"),
        call("c2", ToolName::Exec)?,
        Event::ToolResult {
            call: CallId::new("c2"),
            outcome: ToolOutcome::Timeout,
            delta: None,
        },
        call("c3", ToolName::Read)?,
        ran("c3"),
    ];
    let trials = tool_trials(&events);
    let exec = trials.get(&ToolName::Exec).copied().unwrap_or_default();
    assert_eq!(exec.trials(), 2);
    assert_eq!(exec.successes(), 1);
    let read = trials.get(&ToolName::Read).copied().unwrap_or_default();
    assert_eq!(read.trials(), 1);
    assert_eq!(read.successes(), 1);
    Ok(())
}

fn rule(id: &str, category: RuleCategory) -> Rule {
    Rule {
        id: RuleId::from(id),
        statement: format!("regra {id}"),
        scope: RuleScope::Command {
            tool: ToolName::Exec,
        },
        enforcement: Enforcement::DenyCommand {
            tool: ToolName::Exec,
        },
        severity: Severity::Critical,
        category,
        remedy: Some("não execute".into()),
        expires_at: None,
        waiver: None,
        examples: RuleExamples {
            negative: vec!["exec rm".into()],
            positive: Vec::new(),
        },
    }
}

#[test]
fn the_verdict_covers_only_declared_enforced_rules() -> Result<(), PolicyError> {
    let rules = RuleSet {
        vocab: katu_policy::POLICY_VOCAB_VERSION,
        rules: vec![
            rule("r-enforced", RuleCategory::Enforced),
            rule("r-advisory", RuleCategory::Advisory),
        ],
    };
    let mut events = Vec::new();
    for index in 0..30 {
        let id = format!("c{index}");
        events.push(call(&id, ToolName::Read)?);
        events.push(denied(&id, "r-enforced"));
    }
    let verdicts = enforced_verdicts(&events, &rules, &Threshold::DEFAULT);
    assert_eq!(verdicts.len(), 1, "só a regra Enforced é verificada");
    let Some(first) = verdicts.first() else {
        return Ok(());
    };
    assert!(first.is_proven(), "30 recusas honradas provam a regra");
    assert!(!first.contradiction);
    Ok(())
}

#[test]
fn the_extraction_is_deterministic() -> Result<(), PolicyError> {
    let events = vec![call("c1", ToolName::Read)?, denied("c1", "r1")];
    assert_eq!(rule_trials(&events), rule_trials(&events));
    Ok(())
}
