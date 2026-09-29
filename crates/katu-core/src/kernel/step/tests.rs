use super::step;
use crate::error::ToolOutcome;
use crate::kernel::event::{CallId, Event};
use crate::kernel::state::{CallStatus, Refusal, RefusalReason, State};
use crate::plan::{Feature, FeatureStatus, Plan, ScopeContract};
use katu_policy::{Evidence, Phase, ResolvedPath, RuleId, ToolArgs, ToolName, ToolUse};

/// Plano mínimo válido (E06-T06).
fn plan() -> Plan {
    Plan::new(
        ScopeContract::new(
            Vec::new(),
            vec!["**/secrets/**".to_string()],
            Vec::new(),
            "reverter",
        ),
        vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
    )
}

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

fn read_tool() -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

/// Executa uma tool com sucesso, devolvendo o estado resultante.
fn complete(state: &State, id: &str, tool: ToolUse) -> Result<State, Refusal> {
    let call = CallId::new(id);
    let state = step(
        state,
        &Event::ToolCall {
            call: call.clone(),
            tool,
        },
    )?;
    step(
        &state,
        &Event::ToolResult {
            call,
            outcome: ToolOutcome::Ok,
        },
    )
}

#[test]
fn happy_path_reaches_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::initial();
    state = step(&state, &Event::TurnStart { turn: 1 })?;
    state = step(&state, &Event::UserMessage { text: "oi".into() })?;
    state = complete(&state, "c0", read_tool()?)?;
    state = complete(&state, "c1", tool()?)?;
    state = step(&state, &Event::PlanRecorded { plan: plan() })?;
    for to in [
        Phase::KnowledgeConsulted,
        Phase::Planned,
        Phase::Implemented,
        Phase::Verified,
        Phase::Persisted,
    ] {
        state = step(&state, &Event::PhaseTransition { to, outcome: None })?;
    }
    state = step(
        &state,
        &Event::PhaseTransition {
            to: Phase::Closed,
            outcome: Some("verificado por testes".into()),
        },
    )?;
    state = step(&state, &Event::TurnEnd { turn: 1 })?;
    assert_eq!(state.phase, Phase::Closed);
    assert!(!state.turn_open);
    assert!(state.completed_tools.contains(&ToolName::Write));
    Ok(())
}

#[test]
fn knowledge_consulted_requires_read_or_waiver() -> Result<(), Box<dyn std::error::Error>> {
    let state = State::initial();
    let refused = step(
        &state,
        &Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        },
    );
    assert!(matches!(
        refused,
        Err(refusal) if matches!(
            refusal.reason,
            RefusalReason::UnmetPrecondition {
                to: Phase::KnowledgeConsulted
            }
        )
    ));
    let waived = step(
        &state,
        &Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "sem consulta aplicável".into(),
        },
    )?;
    assert!(waived.waivers.contains(&Phase::KnowledgeConsulted));
    let allowed = step(
        &waived,
        &Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        },
    )?;
    assert_eq!(allowed.phase, Phase::KnowledgeConsulted);
    Ok(())
}

#[test]
fn planned_requires_a_plan() -> Result<(), Box<dyn std::error::Error>> {
    let waived = step(
        &State::initial(),
        &Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "pulo a consulta no teste".into(),
        },
    )?;
    let consulted = step(
        &waived,
        &Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        },
    )?;
    let refused = step(
        &consulted,
        &Event::PhaseTransition {
            to: Phase::Planned,
            outcome: None,
        },
    );
    assert!(matches!(
        refused,
        Err(refusal) if matches!(
            refusal.reason,
            RefusalReason::UnmetPrecondition { to: Phase::Planned }
        )
    ));
    let planned = step(&consulted, &Event::PlanRecorded { plan: plan() })?;
    let allowed = step(
        &planned,
        &Event::PhaseTransition {
            to: Phase::Planned,
            outcome: None,
        },
    )?;
    assert_eq!(allowed.phase, Phase::Planned);
    Ok(())
}

#[test]
fn closed_requires_outcome() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = step(
        &State::initial(),
        &Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "pulo a consulta no teste".into(),
        },
    )?;
    state = step(&state, &Event::PlanRecorded { plan: plan() })?;
    for to in [
        Phase::KnowledgeConsulted,
        Phase::Planned,
        Phase::Implemented,
        Phase::Verified,
        Phase::Persisted,
    ] {
        state = step(&state, &Event::PhaseTransition { to, outcome: None })?;
    }
    for missing in [None, Some("   ".to_string())] {
        let refused = step(
            &state,
            &Event::PhaseTransition {
                to: Phase::Closed,
                outcome: missing,
            },
        );
        assert!(matches!(
            refused,
            Err(refusal) if matches!(
                refusal.reason,
                RefusalReason::UnmetPrecondition { to: Phase::Closed }
            )
        ));
    }
    let closed = step(
        &state,
        &Event::PhaseTransition {
            to: Phase::Closed,
            outcome: Some("evidência".into()),
        },
    )?;
    assert_eq!(closed.phase, Phase::Closed);
    Ok(())
}

#[test]
fn illegal_transition_is_refused() {
    let state = State::initial();
    let result = step(
        &state,
        &Event::PhaseTransition {
            to: Phase::Closed,
            outcome: None,
        },
    );
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
