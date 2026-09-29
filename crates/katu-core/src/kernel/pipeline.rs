//! Pipeline de tool call (E04-T05): facto → política → efeito.
//!
//! Ordem explícita (§42): o `ToolCall` é **logado antes de executar** (responsabilidade do loop);
//! aqui avaliamos a política e só depois executamos. Um `Deny`/`RequireApproval`/`NeedsHuman`
//! **não** executa — o teste prova a negação **pelo executor** (a [`Tool`] não é invocada).

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use katu_policy::{Decision, Facts, PolicyError, RuleSet, ToolName, ToolUse, evaluate};

use super::state::State;

/// Efeito observado de um pedido de tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Effect {
    /// A tool correu (o efeito pode ainda ser `Denied` por contenção, E07).
    Ran {
        /// Efeito da operação.
        outcome: ToolOutcome,
    },
    /// A política recusou: **nada** correu.
    Skipped,
}

/// Resultado do pipeline: veredicto + efeito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    /// Veredicto da política.
    pub decision: Decision,
    /// Efeito observado.
    pub effect: Effect,
}

impl Dispatch {
    /// `true` se a tool foi efetivamente executada.
    #[must_use]
    pub const fn ran(&self) -> bool {
        matches!(self.effect, Effect::Ran { .. })
    }

    /// Efeito a devolver ao modelo (`Denied` quando a política recusou).
    #[must_use]
    pub const fn outcome(&self) -> ToolOutcome {
        match self.effect {
            Effect::Ran { outcome } => outcome,
            Effect::Skipped => ToolOutcome::Denied,
        }
    }
}

/// Tool executável (implementações em `katu-tools`, E06; fakes nos testes).
pub trait Tool {
    /// Nome estável da tool.
    fn name(&self) -> ToolName;

    /// Executa o pedido. Só é chamada quando a política permite.
    fn execute(&self, use_: &ToolUse) -> ToolOutcome;
}

/// Monta os factos que a política avalia, a partir do estado do kernel.
#[must_use]
pub fn facts_for(state: &State, use_: &ToolUse, now_millis: u64) -> Facts {
    Facts {
        now_millis,
        phase: state.phase,
        tool: use_.clone(),
        capabilities: state.capabilities.clone(),
        budget: state.budget,
        completed: state.completed_tools.clone(),
    }
}

/// Avalia a política e, se permitido, executa a tool.
///
/// # Errors
/// [`PolicyError`] se o `RuleSet` tiver vocabulário desconhecido (fail-closed); nesse caso a tool
/// **não** corre.
pub fn dispatch(
    state: &State,
    use_: &ToolUse,
    rules: &RuleSet,
    now_millis: u64,
    tool: &dyn Tool,
) -> Result<Dispatch, PolicyError> {
    let facts = facts_for(state, use_, now_millis);
    let decision = {
        let _span = crate::span!(Level::Trace, events::POLICY_EVALUATE);
        evaluate(&facts, rules)?
    };
    if decision.is_allow() {
        let outcome = tool.execute(use_);
        Ok(Dispatch {
            decision,
            effect: Effect::Ran { outcome },
        })
    } else {
        Ok(Dispatch {
            decision,
            effect: Effect::Skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Tool, dispatch};
    use crate::error::ToolOutcome;
    use crate::kernel::State;
    use katu_policy::{
        Enforcement, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId,
        RuleScope, RuleSet, Severity, ToolArgs, ToolName, ToolUse,
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

        fn execute(&self, _use_: &ToolUse) -> ToolOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.outcome
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
            vocab: 1,
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
        assert_eq!(result.outcome(), ToolOutcome::Denied);
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
            vocab: 1,
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
}
