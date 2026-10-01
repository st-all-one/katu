//! `step(State, Event) -> Result<State, Refusal>` (E04-T01). Transição pura e determinística.

use super::control::Control;
use super::event::{CallId, Event};
use super::state::{Refusal, RefusalReason, State, can_transition};
use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::feedback::{CommandRecord, CommandStatus};
use crate::plan::Plan;
use crate::verify::VerificationReport;
use katu_policy::{Capability, Phase, ResolvedPath, ToolName, ToolUse};

/// Aplica um evento ao estado, devolvendo o novo estado ou uma [`Refusal`].
///
/// `step` é **puro**: não toca relógio, FS nem RNG. Uma recusa não altera o estado (§42).
///
/// # Errors
/// [`Refusal`] se o evento violar a forma do caminho único ou o protocolo de turnos/chamadas.
pub fn step(state: &State, event: &Event) -> Result<State, Refusal> {
    let _span = crate::fn_span!(Level::Trace, events::KERNEL_STEP, "kernel::step::step", "event" => event.kind());
    match event {
        Event::TurnStart { turn } => turn_start(state, *turn),
        // `ProjectContext` (E20-T13/Q-16) é um evento de controlo: não muda a forma do estado,
        // mas exige turno aberto como as mensagens.
        Event::UserMessage { .. }
        | Event::AssistantMessage { .. }
        | Event::ProjectContext { .. }
        | Event::PromptState { .. } => {
            require_open(state)?;
            Ok(state.clone())
        }
        Event::ToolCall { call, tool } => tool_call(state, call, tool),
        Event::ToolResult { call, outcome, .. } => tool_result(state, call, outcome),
        Event::PhaseTransition { to, outcome } => phase_transition(state, *to, outcome.as_deref()),
        Event::Waiver { transition, .. } => Ok(waiver(state, *transition)),
        Event::PlanRecorded { plan } => Ok(plan_recorded(state, plan)),
        Event::CommandRecorded { record } => Ok(command_recorded(state, record)),
        Event::WorkspaceSet { root } => Ok(workspace_set(state, root)),
        Event::ApprovalGranted {
            capability,
            reason,
            granted_by,
            ..
        } => approval_granted(state, capability, reason, granted_by),
        Event::ApprovalRevoked { capability } => Ok(capability_revoked(state, capability)),
        Event::VerificationRecorded { report } => Ok(verification_recorded(state, report)),
        Event::Control { control } => Ok(control_applied(state, control)),
        Event::TurnEnd { turn } => turn_end(state, *turn),
    }
}

/// Abre um turno.
fn turn_start(state: &State, turn: u32) -> Result<State, Refusal> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::KERNEL_TRANSITION,
        "kernel::step::turn_start"
    );
    if state.turn_open {
        return Err(refuse(state, RefusalReason::TurnAlreadyOpen));
    }
    let mut next = state.clone();
    next.turn = turn;
    next.turn_open = true;
    Ok(next)
}

/// Regista um pedido de tool.
fn tool_call(state: &State, call: &CallId, tool: &ToolUse) -> Result<State, Refusal> {
    let _span = crate::trace_fn!("kernel::step::tool_call");

    require_open(state)?;
    // Duplicado **pendente**: dois pedidos vivos com o mesmo id tornariam o resultado ambíguo. Um id
    // já concluído (noutro passo ou turno) é legítimo: `call_0` é o que o provider emite sem id.
    if state.pending.contains_key(call) {
        return Err(refuse(
            state,
            RefusalReason::DuplicateCall { call: call.clone() },
        ));
    }
    let mut next = state.clone();
    next.pending.insert(call.clone(), tool.clone());
    Ok(next)
}

/// Fecha um pedido de tool com o efeito observado.
fn tool_result(state: &State, call: &CallId, outcome: &ToolOutcome) -> Result<State, Refusal> {
    let _span = crate::trace_fn!("kernel::step::tool_result");

    let Some(tool) = state.pending.get(call) else {
        return Err(refuse(
            state,
            RefusalReason::UnknownCall { call: call.clone() },
        ));
    };
    let name = tool.name;
    let mut next = state.clone();
    // O efeito vive no log; aqui só se **fecha** o intervalo pendente.
    next.pending.remove(call);
    if outcome.is_success() {
        next.completed_tools.insert(name);
    }
    Ok(next)
}

/// Regista um `waiver` explícito para uma transição de fase.
fn waiver(state: &State, transition: Phase) -> State {
    let _span = crate::fn_span!(Level::Trace, events::POLICY_WAIVER, "kernel::step::waiver", "phase" => format!("{transition:?}").as_str());
    let mut next = state.clone();
    next.waivers.insert(transition);
    next
}

/// Regista/atualiza o plano do estado (E06-T06).
fn plan_recorded(state: &State, plan: &Plan) -> State {
    let _span = crate::trace_fn!("kernel::step::plan_recorded");

    let mut next = state.clone();
    next.plan = Some(plan.clone());
    next
}

/// Regista o feedback do último comando (E06-T07).
fn command_recorded(state: &State, record: &CommandRecord) -> State {
    let _span = crate::trace_fn!("kernel::step::command_recorded");

    let mut next = state.clone();
    next.last_command = Some(record.status());
    next
}

/// Define a raiz do workspace (E07-T05); a partir daqui a política sabe o que é "fora".
fn workspace_set(state: &State, root: &ResolvedPath) -> State {
    let _span = crate::trace_fn!("kernel::step::workspace_set");

    let mut next = state.clone();
    next.workspace = Some(root.clone());
    next
}

/// Concede uma capacidade aprovada por humano (E07-T05, §33).
///
/// Uma aprovação **sem assinatura** (`reason`/`granted_by` vazios) é recusada: o agente não pode
/// fabricar um override. A capacidade concedida é a mínima derivada da regra.
fn approval_granted(
    state: &State,
    capability: &Capability,
    reason: &str,
    granted_by: &str,
) -> Result<State, Refusal> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::POLICY_APPROVAL,
        "kernel::step::approval_granted"
    );
    if reason.trim().is_empty() || granted_by.trim().is_empty() {
        return Err(refuse(state, RefusalReason::UnsignedApproval));
    }
    let mut next = state.clone();
    if !next.capabilities.contains(capability) {
        next.capabilities.push(capability.clone());
    }
    Ok(next)
}

/// Revoga uma capacidade **one-shot** (B-06): a aprovação de escalação de sandbox não é herdada.
///
/// Remove a capacidade do estado — a próxima escalação exige nova aprovação. É o mecanismo que
/// torna a aprovação **não reutilizável**: depois de usada, a capacidade desaparece.
fn capability_revoked(state: &State, capability: &Capability) -> State {
    let _span = crate::trace_fn!("kernel::step::capability_revoked");

    let mut next = state.clone();
    next.capabilities.retain(|c| c != capability);
    next
}

/// Regista o relatório do gate de verificação (E09-T03).
fn verification_recorded(state: &State, report: &VerificationReport) -> State {
    let _span = crate::trace_fn!("kernel::step::verification_recorded");

    let mut next = state.clone();
    next.verification = Some(report.clone());
    next
}

/// Aplica um controlo **já validado** (E12-T10): só o estado muda (a validação é da borda).
fn control_applied(state: &State, control: &Control) -> State {
    let _span = crate::trace_fn!("kernel::step::control_applied");

    let mut next = state.clone();
    next.control = control.apply(&state.control);
    next
}

/// Muda de fase, validando a forma do caminho único e a pré-condição da fase destino.
fn phase_transition(state: &State, to: Phase, outcome: Option<&str>) -> Result<State, Refusal> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::KERNEL_TRANSITION,
        "kernel::step::phase_transition"
    );
    if !can_transition(state.phase, to) {
        return Err(refuse(
            state,
            RefusalReason::IllegalTransition {
                from: state.phase,
                to,
            },
        ));
    }
    if !satisfies_precondition(state, to, outcome) {
        return Err(refuse(state, RefusalReason::UnmetPrecondition { to }));
    }
    let mut next = state.clone();
    next.phase = to;
    Ok(next)
}

/// Pré-condição da fase destino (E05-T02/T04); um `waiver` dispensa-a (§47).
fn satisfies_precondition(state: &State, to: Phase, outcome: Option<&str>) -> bool {
    let _span = crate::trace_fn!("kernel::step::satisfies_precondition");

    if state.waivers.contains(&to) {
        return true;
    }
    if to == Phase::Task {
        return true;
    }
    if command_ambiguous(state) {
        return false;
    }
    match to {
        Phase::KnowledgeConsulted => {
            state.completed_tools.contains(&ToolName::Read)
                || state.completed_tools.contains(&ToolName::MemoryRecall)
        }
        Phase::Planned => state.plan.is_some(),
        Phase::Verified => state
            .verification
            .as_ref()
            .is_some_and(|report| !report.is_blocked()),
        Phase::Closed => outcome.is_some_and(|evidence| !evidence.trim().is_empty()),
        _ => true,
    }
}

/// `true` se o último comando é **ambíguo** (`exit_code: null`) — bloqueia avançar (§31).
fn command_ambiguous(state: &State) -> bool {
    let _span = crate::trace_fn!("kernel::step::command_ambiguous");

    state.last_command.is_some_and(CommandStatus::is_ambiguous)
}

/// Fecha um turno, validando o número.
fn turn_end(state: &State, turn: u32) -> Result<State, Refusal> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::KERNEL_TRANSITION,
        "kernel::step::turn_end"
    );
    if !state.turn_open {
        return Err(refuse(state, RefusalReason::NoOpenTurn));
    }
    if state.turn != turn {
        return Err(refuse(
            state,
            RefusalReason::TurnMismatch {
                expected: state.turn,
                got: turn,
            },
        ));
    }
    let mut next = state.clone();
    next.turn_open = false;
    Ok(next)
}

/// Exige um turno aberto.
fn require_open(state: &State) -> Result<(), Refusal> {
    let _span = crate::trace_fn!("kernel::step::require_open");

    if state.turn_open {
        Ok(())
    } else {
        Err(refuse(state, RefusalReason::NoOpenTurn))
    }
}

/// Constrói uma recusa ancorada na fase corrente.
fn refuse(state: &State, reason: RefusalReason) -> Refusal {
    let _span = crate::trace_fn!("kernel::step::refuse");

    crate::event!(Level::Warn, events::KERNEL_REFUSAL);
    Refusal {
        reason,
        phase: state.phase,
    }
}

#[cfg(test)]
mod tests;
