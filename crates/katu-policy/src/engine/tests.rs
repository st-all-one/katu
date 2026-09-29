use super::Rule;
use crate::error::PolicyError;
use crate::facts::{BudgetState, Facts, Phase, Timestamp, ToolArgs, ToolName, ToolUse};
use crate::paths::ResolvedPath;
use crate::rule::{Enforcement, RuleCategory, RuleExamples, RuleId, RuleScope, Severity, Waiver};
use std::collections::BTreeSet;

fn deny_write_rule(root: ResolvedPath) -> Rule {
    Rule {
        id: RuleId::from("no-write-secrets"),
        statement: "não escrever em segredos".to_string(),
        scope: RuleScope::Path { root: root.clone() },
        enforcement: Enforcement::DenyWrite { root },
        severity: Severity::Critical,
        category: RuleCategory::Enforced,
        expires_at: None,
        waiver: None,
        examples: RuleExamples::default(),
    }
}

fn facts(path: &str) -> Result<Facts, PolicyError> {
    let resolved = ResolvedPath::from_canonical(path)?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: resolved.clone(),
                bytes: 1,
            },
            resolved_paths: vec![resolved.clone()],
            argv: None,
            cwd: resolved,
        },
        capabilities: Vec::new(),
        budget: BudgetState::default(),
        completed: BTreeSet::new(),
    })
}

#[test]
fn expired_rule_is_inactive() -> Result<(), PolicyError> {
    let root = ResolvedPath::from_canonical("/work/secrets")?;
    let mut rule = deny_write_rule(root);
    rule.expires_at = Some(Timestamp::from_millis(0));
    let facts = facts("/work/secrets/token")?;
    assert!(rule.applies(&facts).is_none());
    Ok(())
}

#[test]
fn waived_rule_is_inactive() -> Result<(), PolicyError> {
    let root = ResolvedPath::from_canonical("/work/secrets")?;
    let mut rule = deny_write_rule(root);
    rule.waiver = Some(Waiver {
        reason: "exceção temporária".to_string(),
        expires_at: None,
    });
    let facts = facts("/work/secrets/token")?;
    assert!(rule.applies(&facts).is_none());
    Ok(())
}
