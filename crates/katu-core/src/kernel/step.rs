//! `step(State, Event) -> Result<State, Refusal>` (E04-T01). Transição pura e determinística.

use super::event::{CallId, Event};
use super::state::{CallStatus, Refusal, RefusalReason, State, can_transition};
use crate::error::ToolOutcome;
use katu_policy::{Phase, ToolUse};

/// Aplica um evento ao estado, devolvendo o novo estado ou uma [`Refusal`].
///
/// `step` é **puro**: não toca relógio, FS nem RNG. Uma recusa não altera o estado (§42).
///
/// # Errors
/// [`Refusal`] se o evento violar a forma do caminho único ou o protocolo de turnos/chamadas.
pub fn step(state: &State, event: &Event) -> Result<State, Refusal> {
    match event {
        Event::TurnStart { turn } => turn_start(state, *turn),
        Event::UserMessage { .. } | Event::AssistantMessage { .. } => {
            require_open(state)?;
            Ok(state.clone())
        }
        Event::ToolCall { call, tool } => tool_call(state, call, tool),
        Event::ToolResult { call, outcome } => tool_result(state, call, outcome),
        Event::PhaseTransition { to } => phase_transition(state, *to),
        Event::TurnEnd { turn } => turn_end(state, *turn),
    }
}

/// Abre um turno.
fn turn_start(state: &State, turn: u32) -> Result<State, Refusal> {
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
    require_open(state)?;
    if state.calls.contains_key(call) {
        return Err(refuse(
            state,
            RefusalReason::DuplicateCall { call: call.clone() },
        ));
    }
    let mut next = state.clone();
    next.calls
        .insert(call.clone(), CallStatus::Pending { tool: tool.clone() });
    Ok(next)
}

/// Fecha um pedido de tool com o efeito observado.
fn tool_result(state: &State, call: &CallId, outcome: &ToolOutcome) -> Result<State, Refusal> {
    let Some(CallStatus::Pending { tool }) = state.calls.get(call) else {
        return Err(refuse(
            state,
            RefusalReason::UnknownCall { call: call.clone() },
        ));
    };
    let name = tool.name;
    let mut next = state.clone();
    next.calls.insert(
        call.clone(),
        CallStatus::Done {
            outcome: outcome.clone(),
        },
    );
    if outcome.is_success() {
        next.completed_tools.insert(name);
    }
    Ok(next)
}

/// Muda de fase, validando a forma do caminho único.
fn phase_transition(state: &State, to: Phase) -> Result<State, Refusal> {
    if !can_transition(state.phase, to) {
        return Err(refuse(
            state,
            RefusalReason::IllegalTransition {
                from: state.phase,
                to,
            },
        ));
    }
    let mut next = state.clone();
    next.phase = to;
    Ok(next)
}

/// Fecha um turno, validando o número.
fn turn_end(state: &State, turn: u32) -> Result<State, Refusal> {
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
    if state.turn_open {
        Ok(())
    } else {
        Err(refuse(state, RefusalReason::NoOpenTurn))
    }
}

/// Constrói uma recusa ancorada na fase corrente.
fn refuse(state: &State, reason: RefusalReason) -> Refusal {
    Refusal {
        reason,
        phase: state.phase,
    }
}

#[cfg(test)]
mod tests {
    use super::step;
    use crate::error::ToolOutcome;
    use crate::kernel::event::{CallId, Event};
    use crate::kernel::state::{CallStatus, RefusalReason, State};
    use katu_policy::{Evidence, Phase, ResolvedPath, RuleId, ToolArgs, ToolName, ToolUse};

    fn denied_outcome() -> ToolOutcome {
        let rule_id = RuleId::from("test");
        ToolOutcome::Denied {
            evidence: Evidence::new("facto", "argumento", rule_id.clone()),
            rule_id,
        }
    }

    fn tool() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
        Ok(ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: path.clone(),
                bytes: 1,
            },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    #[test]
    fn happy_path_reaches_closed() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = State::initial();
        state = step(&state, &Event::TurnStart { turn: 1 })?;
        state = step(&state, &Event::UserMessage { text: "oi".into() })?;
        let call = CallId::new("c1");
        state = step(
            &state,
            &Event::ToolCall {
                call: call.clone(),
                tool: tool()?,
            },
        )?;
        state = step(
            &state,
            &Event::ToolResult {
                call,
                outcome: ToolOutcome::Ok,
            },
        )?;
        for to in [
            Phase::KnowledgeConsulted,
            Phase::Planned,
            Phase::Implemented,
            Phase::Verified,
            Phase::Persisted,
            Phase::Closed,
        ] {
            state = step(&state, &Event::PhaseTransition { to })?;
        }
        state = step(&state, &Event::TurnEnd { turn: 1 })?;
        assert_eq!(state.phase, Phase::Closed);
        assert!(!state.turn_open);
        assert!(state.completed_tools.contains(&ToolName::Write));
        Ok(())
    }

    #[test]
    fn illegal_transition_is_refused() {
        let state = State::initial();
        let result = step(&state, &Event::PhaseTransition { to: Phase::Closed });
        assert!(matches!(
            result,
            Err(refusal) if matches!(refusal.reason, RefusalReason::IllegalTransition { .. })
        ));
    }

    #[test]
    fn duplicate_call_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let call = CallId::new("c1");
        let mut state = State::initial();
        state = step(&state, &Event::TurnStart { turn: 1 })?;
        state = step(
            &state,
            &Event::ToolCall {
                call: call.clone(),
                tool: tool()?,
            },
        )?;
        let again = step(
            &state,
            &Event::ToolCall {
                call,
                tool: tool()?,
            },
        );
        assert!(matches!(
            again,
            Err(refusal) if matches!(refusal.reason, RefusalReason::DuplicateCall { .. })
        ));
        Ok(())
    }

    #[test]
    fn denied_result_does_not_complete_tool() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = State::initial();
        state = step(&state, &Event::TurnStart { turn: 1 })?;
        let call = CallId::new("c1");
        state = step(
            &state,
            &Event::ToolCall {
                call: call.clone(),
                tool: tool()?,
            },
        )?;
        state = step(
            &state,
            &Event::ToolResult {
                call,
                outcome: denied_outcome(),
            },
        )?;
        assert!(state.completed_tools.is_empty());
        assert!(matches!(
            state.calls.get(&CallId::new("c1")),
            Some(CallStatus::Done {
                outcome: ToolOutcome::Denied { .. }
            })
        ));
        Ok(())
    }
}
