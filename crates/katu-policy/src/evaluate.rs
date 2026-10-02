//! Motor `evaluate` **puro** (E02-T03): factos tipados → veredicto determinístico.
//!
//! Ordem de custo crescente (§32): âmbito → exemplo negativo → orçamento. Sem I/O, sem relógio, sem
//! regex sobre texto. O veredicto de maior [`Decision::rank`] vence.

use crate::decision::Decision;
use crate::error::PolicyError;
use crate::facts::Facts;
use crate::rule::{RuleCategory, RuleSet};

/// Avalia os factos contra o `RuleSet`, devolvendo o veredicto mais forte.
///
/// # Errors
/// Devolve [`PolicyError::UnknownVocab`] se o `RuleSet` tiver um vocabulário desconhecido
/// (fail-closed).
pub fn evaluate(facts: &Facts, rules: &RuleSet) -> Result<Decision, PolicyError> {
    rules.check_vocab()?;
    let mut best = Decision::Allow;
    for rule in &rules.rules {
        // Nem `Advisory` nem `Perception` decidem: o motor só aplica `Enforced` (o audit
        // classifica as duas primeiras como não-decisórias).
        if matches!(
            rule.category,
            RuleCategory::Advisory | RuleCategory::Perception
        ) {
            continue;
        }
        let Some(evidence) = rule.applies(facts) else {
            continue;
        };
        let candidate = rule.verdict(evidence);
        if candidate.rank() > best.rank() {
            best = candidate;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::evaluate;
    use crate::decision::Decision;
    use crate::error::PolicyError;
    use crate::facts::{BudgetState, Facts, Phase, ToolArgs, ToolName, ToolUse};
    use crate::paths::ResolvedPath;
    use crate::rule::{
        BudgetCap, Enforcement, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet,
        Severity,
    };
    use std::collections::BTreeSet;

    fn facts_for(tool: ToolName, args: ToolArgs, path: &str) -> Result<Facts, PolicyError> {
        let resolved = ResolvedPath::from_canonical(path)?;
        Ok(Facts {
            now_millis: 1_000,
            phase: Phase::Task,
            tool: ToolUse {
                name: tool,
                args,
                resolved_paths: vec![resolved.clone()],
                argv: None,
                cwd: resolved,
            },
            capabilities: Vec::new(),
            budget: BudgetState::default(),
            completed: BTreeSet::new(),
        })
    }

    fn rule(id: &str, scope: RuleScope, enforcement: Enforcement) -> Rule {
        Rule {
            id: RuleId::from(id),
            statement: id.to_string(),
            scope,
            enforcement,
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            remedy: None,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }
    }

    fn write_facts(path: &str) -> Result<Facts, PolicyError> {
        facts_for(
            ToolName::Write,
            ToolArgs::Write {
                path: ResolvedPath::from_canonical(path)?,
                bytes: 10,
            },
            path,
        )
    }

    #[test]
    fn denies_write_under_root() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work/secrets")?;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule(
                "no-secrets",
                RuleScope::Path { root: root.clone() },
                Enforcement::DenyWrite { root },
            )],
        };
        let facts = write_facts("/work/secrets/token")?;
        assert!(matches!(evaluate(&facts, &rules)?, Decision::Deny { .. }));
        Ok(())
    }

    #[test]
    fn allows_write_outside_root() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work/secrets")?;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule(
                "no-secrets",
                RuleScope::Path { root: root.clone() },
                Enforcement::DenyWrite { root },
            )],
        };
        let facts = write_facts("/work/src/main.rs")?;
        assert!(evaluate(&facts, &rules)?.is_allow());
        Ok(())
    }

    #[test]
    fn denies_write_outside_the_allowed_root() -> Result<(), PolicyError> {
        let allowed = ResolvedPath::from_canonical("/work/.katu")?;
        let anywhere = ResolvedPath::from_canonical("/")?;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule(
                "plan-write-only-katu",
                RuleScope::Path { root: anywhere },
                Enforcement::DenyWriteOutside { root: allowed },
            )],
        };
        assert!(matches!(
            evaluate(&write_facts("/work/src/main.rs")?, &rules)?,
            Decision::Deny { .. }
        ));
        assert!(evaluate(&write_facts("/work/.katu/plan/x.md")?, &rules)?.is_allow());
        Ok(())
    }

    #[test]
    fn critical_require_after_denies() -> Result<(), PolicyError> {
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule(
                "need-verify",
                RuleScope::Command {
                    tool: ToolName::Write,
                },
                Enforcement::RequireAfter {
                    tool: ToolName::Search,
                },
            )],
        };
        let facts = write_facts("/work/x")?;
        assert!(matches!(evaluate(&facts, &rules)?, Decision::Deny { .. }));
        Ok(())
    }

    #[test]
    fn warn_require_after_requires_approval() -> Result<(), PolicyError> {
        let mut require = rule(
            "need-verify",
            RuleScope::Command {
                tool: ToolName::Write,
            },
            Enforcement::RequireAfter {
                tool: ToolName::Search,
            },
        );
        require.severity = Severity::Warn;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![require],
        };
        let facts = write_facts("/work/x")?;
        assert!(matches!(
            evaluate(&facts, &rules)?,
            Decision::RequireApproval { .. }
        ));
        Ok(())
    }

    #[test]
    fn budget_exhausted_needs_human() -> Result<(), PolicyError> {
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule(
                "budget",
                RuleScope::Budget {
                    cap: BudgetCap::Writes(5),
                },
                Enforcement::Budget {
                    cap: BudgetCap::Writes(5),
                },
            )],
        };
        let mut facts = write_facts("/work/x")?;
        facts.budget = BudgetState {
            writes: 5,
            bytes: 0,
            execs: 0,
        };
        assert!(matches!(
            evaluate(&facts, &rules)?,
            Decision::NeedsHuman { .. }
        ));
        Ok(())
    }

    #[test]
    fn warn_severity_requires_approval() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work")?;
        let mut deny = rule(
            "warn-write",
            RuleScope::Path { root: root.clone() },
            Enforcement::DenyWrite { root },
        );
        deny.severity = Severity::Warn;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![deny],
        };
        let facts = write_facts("/work/x")?;
        assert!(matches!(
            evaluate(&facts, &rules)?,
            Decision::RequireApproval { .. }
        ));
        Ok(())
    }

    #[test]
    fn evaluation_is_deterministic() -> Result<(), PolicyError> {
        let rules = RuleSet {
            vocab: 3,
            rules: Vec::new(),
        };
        let facts = facts_for(ToolName::Read, ToolArgs::Plan, "/work/x")?;
        assert_eq!(evaluate(&facts, &rules)?, evaluate(&facts, &rules)?);
        Ok(())
    }

    #[test]
    fn unknown_vocab_fails_closed() -> Result<(), PolicyError> {
        let rules = RuleSet {
            vocab: 42,
            rules: Vec::new(),
        };
        let facts = facts_for(ToolName::Read, ToolArgs::Plan, "/work/x")?;
        assert!(evaluate(&facts, &rules).is_err());
        Ok(())
    }

    #[test]
    fn advisory_never_decides() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work")?;
        let mut advisory = rule(
            "informa",
            RuleScope::Path { root: root.clone() },
            Enforcement::DenyWrite { root },
        );
        advisory.category = RuleCategory::Advisory;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![advisory],
        };
        assert!(evaluate(&write_facts("/work/x")?, &rules)?.is_allow());
        Ok(())
    }

    #[test]
    fn perception_never_decides() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work")?;
        let mut perception = rule(
            "percebe",
            RuleScope::Path { root: root.clone() },
            Enforcement::DenyWrite { root },
        );
        perception.category = RuleCategory::Perception;
        let rules = RuleSet {
            vocab: 3,
            rules: vec![perception],
        };
        assert!(
            evaluate(&write_facts("/work/x")?, &rules)?.is_allow(),
            "uma regra Perception não decide, mesmo com enforcement não-Advisory"
        );
        Ok(())
    }
}
