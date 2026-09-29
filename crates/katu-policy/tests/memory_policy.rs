//! E02-T07 (gate do épico) — o protocolo de memória do knudge é **expressável** no vocabulário
//! fechado. Carrega o artefacto real `policy/memory.toml` e prova que:
//! - todas as regras são `Enforced` e a lista `Advisory` do protocolo fica **vazia**;
//! - as 5 regras decidem como o protocolo exige (recall→write, outcome→close, sem capacidade→deny).

use std::collections::BTreeSet;

use katu_policy::{
    BudgetState, Capability, Decision, Facts, Phase, PolicyError, ResolvedPath, RuleSet, ToolArgs,
    ToolName, ToolUse, audit, evaluate,
};

/// Regras reais do protocolo, versionadas no repositório.
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

fn facts(
    tool: ToolName,
    completed: &[ToolName],
    capabilities: Vec<Capability>,
) -> Result<Facts, PolicyError> {
    let cwd = ResolvedPath::from_canonical("/work")?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: tool,
            args: ToolArgs::Other,
            resolved_paths: Vec::new(),
            argv: None,
            cwd,
        },
        capabilities,
        budget: BudgetState::default(),
        completed: completed.iter().copied().collect::<BTreeSet<_>>(),
    })
}

fn command(tool: ToolName) -> Vec<Capability> {
    vec![Capability::Command { tool }]
}

fn memory_rules() -> Result<RuleSet, PolicyError> {
    RuleSet::from_toml(MEMORY_POLICY)
}

#[test]
fn memory_policy_is_all_enforced_and_advisory_free() -> Result<(), PolicyError> {
    let report = audit(&memory_rules()?, 0);
    assert!(report.is_clean(), "audit: {:?}", report.issues);
    assert_eq!(report.enforced.len(), 5, "5 regras Enforced esperadas");
    assert!(
        report.advisory.is_empty(),
        "protocolo não pode ter Advisory"
    );
    Ok(())
}

#[test]
fn write_with_recall_and_capability_is_allowed() -> Result<(), PolicyError> {
    let rules = memory_rules()?;
    let facts = facts(
        ToolName::MemoryWrite,
        &[ToolName::MemoryRecall],
        command(ToolName::MemoryWrite),
    )?;
    assert!(evaluate(&facts, &rules)?.is_allow());
    Ok(())
}

#[test]
fn write_without_recall_is_denied() -> Result<(), PolicyError> {
    let rules = memory_rules()?;
    let facts = facts(ToolName::MemoryWrite, &[], command(ToolName::MemoryWrite))?;
    let decision = evaluate(&facts, &rules)?;
    assert!(matches!(
        decision,
        Decision::Deny { ref rule_id, .. } if rule_id.as_str() == "mem-recall-before-write"
    ));
    Ok(())
}

#[test]
fn write_without_capability_is_denied() -> Result<(), PolicyError> {
    let rules = memory_rules()?;
    let facts = facts(ToolName::MemoryWrite, &[ToolName::MemoryRecall], Vec::new())?;
    assert!(matches!(evaluate(&facts, &rules)?, Decision::Deny { .. }));
    Ok(())
}

#[test]
fn close_without_outcome_is_denied() -> Result<(), PolicyError> {
    let rules = memory_rules()?;
    let facts = facts(ToolName::MemoryClose, &[], command(ToolName::MemoryClose))?;
    let decision = evaluate(&facts, &rules)?;
    assert!(matches!(
        decision,
        Decision::Deny { ref rule_id, .. } if rule_id.as_str() == "mem-outcome-before-close"
    ));
    Ok(())
}

#[test]
fn close_with_outcome_is_allowed() -> Result<(), PolicyError> {
    let rules = memory_rules()?;
    let facts = facts(
        ToolName::MemoryClose,
        &[ToolName::MemoryOutcome],
        command(ToolName::MemoryClose),
    )?;
    assert!(evaluate(&facts, &rules)?.is_allow());
    Ok(())
}
