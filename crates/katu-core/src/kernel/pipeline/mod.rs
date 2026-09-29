//! Pipeline de tool call (E04-T05): facto → política → efeito.
//!
//! Ordem explícita (§42): o `ToolCall` é **logado antes de executar** (responsabilidade do loop);
//! aqui avaliamos a política e só depois executamos. Um `Deny`/`RequireApproval`/`NeedsHuman`
//! **não** executa — o teste prova a negação **pelo executor** (a [`Tool`] não é invocada).

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use katu_policy::{
    Capability, ControlId, Decision, Facts, PolicyError, RuleSet, ToolName, ToolUse, evaluate,
};

use super::state::State;

/// Efeito observado de um pedido de tool.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        matches!(&self.effect, Effect::Ran { .. })
    }

    /// Efeito a devolver ao modelo: o da execução, ou o derivado do veredicto quando nada correu.
    #[must_use]
    pub fn outcome(&self) -> ToolOutcome {
        match &self.effect {
            Effect::Ran { outcome } => outcome.clone(),
            Effect::Skipped => skipped_outcome(&self.decision),
        }
    }
}

/// Traduz um veredicto não-`Allow` num resultado de tool (recuperável).
fn skipped_outcome(decision: &Decision) -> ToolOutcome {
    match decision {
        Decision::Deny {
            rule_id, evidence, ..
        } => ToolOutcome::Denied {
            rule_id: rule_id.clone(),
            evidence: evidence.clone(),
        },
        Decision::RequireApproval { .. } => ToolOutcome::Unavailable {
            control: ControlId::new("approval"),
        },
        Decision::NeedsHuman {
            missing_control, ..
        } => ToolOutcome::Unavailable {
            control: missing_control.clone(),
        },
        _ => ToolOutcome::Unavailable {
            control: ControlId::new("unknown"),
        },
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
    facts_from(state, use_, now_millis, &state.capabilities)
}

/// Monta os factos com **capacidades explícitas** (ex.: concedidas por `pre_write`, E05-T01).
#[must_use]
pub fn facts_from(
    state: &State,
    use_: &ToolUse,
    now_millis: u64,
    capabilities: &[Capability],
) -> Facts {
    Facts {
        now_millis,
        phase: state.phase,
        tool: use_.clone(),
        capabilities: capabilities.to_vec(),
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
    dispatch_with(DispatchRequest {
        state,
        use_,
        rules,
        now_millis,
        capabilities: &state.capabilities,
        tool,
    })
}

/// Pedido de dispatch com **capacidades explícitas** (ex.: concedidas por `pre_write`, E05-T01).
#[derive(Clone, Copy)]
pub struct DispatchRequest<'a> {
    /// Estado do kernel.
    pub state: &'a State,
    /// Uso de tool resolvido.
    pub use_: &'a ToolUse,
    /// Regras a avaliar.
    pub rules: &'a RuleSet,
    /// Instante corrente (ms desde a época).
    pub now_millis: u64,
    /// Capacidades concedidas no contexto corrente.
    pub capabilities: &'a [Capability],
    /// Executor do efeito.
    pub tool: &'a dyn Tool,
}

/// Como [`dispatch`], mas com as capacidades fornecidas pelo chamador.
///
/// # Errors
/// [`PolicyError`] se o `RuleSet` tiver vocabulário desconhecido (fail-closed).
pub fn dispatch_with(request: DispatchRequest<'_>) -> Result<Dispatch, PolicyError> {
    let facts = facts_from(
        request.state,
        request.use_,
        request.now_millis,
        request.capabilities,
    );
    let decision = {
        let _span = crate::span!(Level::Trace, events::POLICY_EVALUATE);
        evaluate(&facts, request.rules)?
    };
    if decision.is_allow() {
        crate::event!(Level::Debug, events::POLICY_ALLOW);
        let outcome = request.tool.execute(request.use_);
        Ok(Dispatch {
            decision,
            effect: Effect::Ran { outcome },
        })
    } else {
        crate::event!(Level::Warn, events::POLICY_DENY);
        Ok(Dispatch {
            decision,
            effect: Effect::Skipped,
        })
    }
}

#[cfg(test)]
mod tests;
