//! Pipeline de tool call (E04-T05): facto → política → efeito.
//!
//! Ordem explícita (§42): o `ToolCall` é **logado antes de executar** (responsabilidade do loop);
//! aqui avaliamos a política e só depois executamos. Um `Deny`/`RequireApproval`/`NeedsHuman`
//! **não** executa — o teste prova a negação **pelo executor** (a [`Tool`] não é invocada).

use crate::containment::workspace_capabilities;
use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::report::{Cost, ToolReport};
use katu_policy::{
    Capability, ControlId, Decision, Facts, PolicyError, RuleSet, ToolName, ToolUse, evaluate,
};

use super::state::State;

/// Resultado de uma execução de tool: estado + payload (envelope, DF12).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    /// Estado (Ok/Partial/Denied/…).
    pub outcome: ToolOutcome,
    /// Envelope de sucesso (opcional; ausente em recusas).
    pub report: Option<ToolReport>,
}

impl ToolOutput {
    /// Saída sem payload.
    #[must_use]
    pub const fn outcome(outcome: ToolOutcome) -> Self {
        Self {
            outcome,
            report: None,
        }
    }

    /// Saída `Ok` sem payload.
    #[must_use]
    pub const fn ok() -> Self {
        Self {
            outcome: ToolOutcome::Ok,
            report: None,
        }
    }

    /// Saída `Ok` com envelope.
    #[must_use]
    pub fn report(report: ToolReport) -> Self {
        let _span = crate::trace_fn!("kernel::pipeline::report");

        Self {
            outcome: ToolOutcome::Ok,
            report: Some(report),
        }
    }
}

/// Efeito observado de um pedido de tool.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Effect {
    /// A tool correu (o efeito pode ainda ser `Denied` por contenção, E07).
    Ran {
        /// Saída da operação (estado + envelope), boxed para manter o enum pequeno.
        output: Box<ToolOutput>,
    },
    /// A política recusou: **nada** correu.
    Skipped,
}

/// Resultado do pipeline: veredicto + efeito.
#[derive(Debug, Clone, PartialEq)]
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
        let _span = crate::trace_fn!("kernel::pipeline::outcome");

        match &self.effect {
            Effect::Ran { output } => output.outcome.clone(),
            Effect::Skipped => skipped_outcome(&self.decision),
        }
    }

    /// Envelope de sucesso, quando a tool correu e o produziu (DF12/E06-T12).
    #[must_use]
    pub fn report(&self) -> Option<&ToolReport> {
        let _span = crate::trace_fn!("kernel::pipeline::report");

        match &self.effect {
            Effect::Ran { output } => output.report.as_ref(),
            Effect::Skipped => None,
        }
    }

    /// Delta **model-visible** do efeito: o TOON do envelope, cortado ao teto (§18/G6).
    ///
    /// É o texto que entra no log (`ToolResult.delta`) e no pedido ao modelo — sem ele, o modelo
    /// recebia apenas `{"Ok":null}` e ficava **cego** ao que a tool devolveu.
    #[must_use]
    pub fn delta(&self) -> Option<String> {
        let _span = crate::trace_fn!("kernel::pipeline::delta");

        self.report().map(ToolReport::to_delta)
    }
}

/// Traduz um veredicto não-`Allow` num resultado de tool (recuperável).
fn skipped_outcome(decision: &Decision) -> ToolOutcome {
    let _span = crate::trace_fn!("kernel::pipeline::skipped_outcome");

    match decision {
        Decision::Deny {
            rule_id, evidence, ..
        } => ToolOutcome::Denied {
            rule_id: rule_id.clone(),
            evidence: evidence.clone(),
        },
        Decision::RequireApproval { request } => ToolOutcome::Unavailable {
            control: ControlId::new("approval"),
            rule_id: Some(request.rule_id.clone()),
        },
        Decision::NeedsHuman {
            missing_control, ..
        } => ToolOutcome::Unavailable {
            control: missing_control.clone(),
            rule_id: None,
        },
        _ => ToolOutcome::Unavailable {
            control: ControlId::new("unknown"),
            rule_id: None,
        },
    }
}

/// Tool executável (implementações em `katu-tools`, E06; fakes nos testes).
///
/// `Send + Sync`: uma tool classificada `Shared` executa num worker paralelo (B-01) e o efeito
/// tem de poder atravessar threads. Todas as implementações só seguram portas (`Fs`/`Process`/
/// `Env`/`Clock`/`Memory`, todas `Send + Sync`) e dados próprios.
pub trait Tool: Send + Sync {
    /// Nome estável da tool.
    fn name(&self) -> ToolName;

    /// Executa o pedido. Só é chamada quando a política permite.
    fn execute(&self, use_: &ToolUse) -> ToolOutput;
}

/// Monta os factos que a política avalia, a partir do estado do kernel.
#[must_use]
pub fn facts_for(state: &State, use_: &ToolUse, now_millis: u64) -> Facts {
    let _span = crate::trace_fn!("kernel::pipeline::facts_for");

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
    let _span = crate::trace_fn!("kernel::pipeline::facts_from");

    let mut granted = capabilities.to_vec();
    if let Some(root) = &state.workspace {
        granted.extend(workspace_capabilities(root));
    }
    Facts {
        now_millis,
        phase: state.phase,
        tool: use_.clone(),
        capabilities: granted,
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
    let _span = crate::trace_fn!("kernel::pipeline::dispatch");

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
    let _span = crate::trace_fn!("kernel::pipeline::dispatch_with");

    let facts = facts_from(
        request.state,
        request.use_,
        request.now_millis,
        request.capabilities,
    );
    let decision = {
        // E02-T04/S-03: a política é instrumentada **pelo chamador** (firewall); o rótulo atribui
        // o custo a `policy::evaluate` sem a `katu-policy` depender do `diag`.
        let _span = crate::fn_span!(Level::Trace, events::POLICY_EVALUATE, "policy::evaluate");
        evaluate(&facts, request.rules)?
    };
    if decision.is_allow() {
        crate::event!(Level::Debug, events::POLICY_ALLOW);
        let output = request.tool.execute(request.use_);
        Ok(Dispatch {
            decision,
            effect: Effect::Ran {
                output: Box::new(with_estimated_cost(output)),
            },
        })
    } else {
        crate::event!(Level::Warn, events::POLICY_DENY);
        Ok(Dispatch {
            decision,
            effect: Effect::Skipped,
        })
    }
}

/// Preenche o custo **advisory** (bytes/tokens estimados) do envelope quando a tool não o fez.
///
/// O custo nunca decide nada (DF5): é uma heurística para o modelo orçamentar a leitura. Os bytes
/// contam a renderização TOON e os tokens são estimados a 4 bytes/token.
fn with_estimated_cost(mut output: ToolOutput) -> ToolOutput {
    let _span = crate::trace_fn!("kernel::pipeline::with_estimated_cost");

    if let Some(report) = output.report.as_mut()
        && report.cost.is_none()
    {
        let bytes = u64::try_from(report.to_toon().len()).unwrap_or(u64::MAX);
        report.cost = Some(Cost {
            bytes,
            ms: 0,
            tokens_est: bytes.div_ceil(4),
        });
    }
    output
}

#[cfg(test)]
mod tests;
