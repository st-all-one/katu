use super::{complete, pass_report, plan, read_tool, step, tool};
use crate::feedback::CommandRecord;
use crate::kernel::event::Event;
use crate::kernel::state::{RefusalReason, State};
use katu_policy::{Phase, ResolvedPath, ToolName};

#[test]
fn happy_path_reaches_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::initial();
    state = step(&state, &Event::TurnStart { turn: 1 })?;
    state = step(&state, &Event::UserMessage { text: "oi".into() })?;
    state = complete(&state, "c0", read_tool()?)?;
    state = complete(&state, "c1", tool()?)?;
    state = step(&state, &Event::PlanRecorded { plan: plan() })?;
    state = step(
        &state,
        &Event::VerificationRecorded {
            report: pass_report(),
        },
    )?;
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

/// Registo de comando mínimo (E06-T07).
fn command(exit_code: Option<i32>) -> CommandRecord {
    CommandRecord {
        id: "x_1".to_string(),
        argv: vec!["echo".to_string()],
        cwd: "/work".to_string(),
        exit_code,
        signal: None,
        timed_out: false,
        duration_ms: 1,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        stdout_spill: None,
        stderr_spill: None,
        parent_command_id: None,
    }
}

#[test]
fn ambiguous_command_blocks_advance() -> Result<(), Box<dyn std::error::Error>> {
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
    let blocked = step(
        &consulted,
        &Event::CommandRecorded {
            record: command(None),
        },
    )?;
    let refused = step(
        &blocked,
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
    let resolved = step(
        &consulted,
        &Event::CommandRecorded {
            record: command(Some(0)),
        },
    )?;
    let planned = step(&resolved, &Event::PlanRecorded { plan: plan() })?;
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
fn workspace_set_is_recorded() -> Result<(), Box<dyn std::error::Error>> {
    let root = ResolvedPath::from_canonical("/work")?;
    let state = step(
        &State::initial(),
        &Event::WorkspaceSet { root: root.clone() },
    )?;
    assert_eq!(state.workspace.as_ref(), Some(&root));
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
    state = step(
        &state,
        &Event::VerificationRecorded {
            report: pass_report(),
        },
    )?;
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
