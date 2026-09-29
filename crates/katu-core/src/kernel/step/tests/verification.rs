//! Pré-condição da fase `Verified` (E09-T03): exige um relatório não bloqueado.

use super::{pass_report, plan, step};
use crate::kernel::event::Event;
use crate::kernel::state::{RefusalReason, State};
use crate::verify::CheckStatus;
use katu_policy::Phase;

#[test]
fn verified_requires_a_non_blocked_report() -> Result<(), Box<dyn std::error::Error>> {
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
    ] {
        state = step(&state, &Event::PhaseTransition { to, outcome: None })?;
    }
    let refused = step(
        &state,
        &Event::PhaseTransition {
            to: Phase::Verified,
            outcome: None,
        },
    );
    assert!(matches!(
        refused,
        Err(refusal) if matches!(
            refusal.reason,
            RefusalReason::UnmetPrecondition { to: Phase::Verified }
        )
    ));
    let mut blocked = pass_report();
    blocked.status = CheckStatus::Block;
    let blocked_state = step(&state, &Event::VerificationRecorded { report: blocked })?;
    assert!(
        step(
            &blocked_state,
            &Event::PhaseTransition {
                to: Phase::Verified,
                outcome: None,
            },
        )
        .is_err(),
        "relatório bloqueado não destranca `Verified`"
    );
    let passed = step(
        &state,
        &Event::VerificationRecorded {
            report: pass_report(),
        },
    )?;
    let verified = step(
        &passed,
        &Event::PhaseTransition {
            to: Phase::Verified,
            outcome: None,
        },
    )?;
    assert_eq!(verified.phase, Phase::Verified);
    Ok(())
}
