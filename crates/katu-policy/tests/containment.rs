//! E07-T05 — contenção soft: sensíveis `deny`-by-default e fora do workspace sob aprovação.
//!
//! Carrega o `policy/containment.toml` **real** (não um `RuleSet` inline) para o teste provar a
//! regra que corre em produção. A distinção-chave: `Capability::Workspace` (implícita) destranca o
//! normal dentro da raiz, mas **nunca** um caminho sensível — esse exige `ReadPath` explícito.

use std::collections::BTreeSet;

use katu_policy::{
    BudgetState, Capability, Decision, Facts, Phase, PolicyError, ResolvedPath, RuleSet, ToolArgs,
    ToolName, ToolUse, evaluate,
};

const CONTAINMENT: &str = include_str!("../../../policy/containment.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Allow,
    Deny,
    RequireApproval,
    NeedsHuman,
}

fn classify(decision: &Decision) -> Verdict {
    match decision {
        Decision::Allow => Verdict::Allow,
        Decision::RequireApproval { .. } => Verdict::RequireApproval,
        Decision::NeedsHuman { .. } => Verdict::NeedsHuman,
        _ => Verdict::Deny,
    }
}

fn rules() -> Result<RuleSet, PolicyError> {
    RuleSet::from_toml(CONTAINMENT)
}

fn read_facts(target: &str, capabilities: Vec<Capability>) -> Result<Facts, PolicyError> {
    let resolved = ResolvedPath::from_canonical(target)?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: ToolName::Read,
            args: ToolArgs::Read {
                path: resolved.clone(),
            },
            resolved_paths: vec![resolved.clone()],
            argv: None,
            cwd: resolved,
        },
        capabilities,
        budget: BudgetState::default(),
        completed: BTreeSet::new(),
    })
}

fn write_facts(target: &str, capabilities: Vec<Capability>) -> Result<Facts, PolicyError> {
    let resolved = ResolvedPath::from_canonical(target)?;
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
        capabilities,
        budget: BudgetState::default(),
        completed: BTreeSet::new(),
    })
}

fn workspace(root: &str) -> Result<Capability, PolicyError> {
    Ok(Capability::Workspace {
        root: ResolvedPath::from_canonical(root)?,
    })
}

fn explicit_read(root: &str) -> Result<Capability, PolicyError> {
    Ok(Capability::ReadPath {
        root: ResolvedPath::from_canonical(root)?,
    })
}

#[test]
fn sensitive_read_is_denied_without_authorization() -> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let facts = read_facts("/home/u/.ssh/id_rsa", Vec::new())?;
    assert_eq!(classify(&evaluate(&facts, &rules)?), Verdict::Deny);
    let env = read_facts("/home/u/.env", Vec::new())?;
    assert_eq!(classify(&evaluate(&env, &rules)?), Verdict::Deny);
    Ok(())
}

#[test]
fn workspace_does_not_unlock_sensitive_read() -> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let facts = read_facts("/work/.env", vec![workspace("/work")?])?;
    assert_eq!(
        classify(&evaluate(&facts, &rules)?),
        Verdict::Deny,
        "o workspace nunca destranca um caminho sensível"
    );
    Ok(())
}

#[test]
fn explicit_read_path_unlocks_sensitive_read() -> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let facts = read_facts("/work/.env", vec![explicit_read("/work")?])?;
    assert_eq!(classify(&evaluate(&facts, &rules)?), Verdict::Allow);
    Ok(())
}

#[test]
fn normal_read_inside_workspace_is_allowed() -> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let facts = read_facts("/work/src/main.rs", vec![workspace("/work")?])?;
    assert_eq!(classify(&evaluate(&facts, &rules)?), Verdict::Allow);
    Ok(())
}

#[test]
fn read_outside_workspace_requires_approval() -> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let facts = read_facts("/etc/passwd", vec![workspace("/work")?])?;
    assert_eq!(
        classify(&evaluate(&facts, &rules)?),
        Verdict::RequireApproval
    );
    Ok(())
}

#[test]
fn write_inside_workspace_is_allowed_and_outside_needs_approval()
-> Result<(), Box<dyn std::error::Error>> {
    let rules = rules()?;
    let inside = write_facts("/work/src/x.rs", vec![workspace("/work")?])?;
    assert_eq!(classify(&evaluate(&inside, &rules)?), Verdict::Allow);
    let outside = write_facts("/etc/hosts", vec![workspace("/work")?])?;
    assert_eq!(
        classify(&evaluate(&outside, &rules)?),
        Verdict::RequireApproval
    );
    Ok(())
}
