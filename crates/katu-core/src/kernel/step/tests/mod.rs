use super::step;
use crate::error::ToolOutcome;
use crate::kernel::event::{CallId, Event};
use crate::kernel::state::{CallStatus, Refusal, RefusalReason, State};
use crate::plan::{Feature, FeatureStatus, Plan, ScopeContract};
use crate::verify::{CheckStatus, VERIFICATION_SCHEMA_VERSION, VerificationReport};
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

/// Relatório de verificação que passa (E09-T03), para as transições de teste.
fn pass_report() -> VerificationReport {
    VerificationReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        checks: Vec::new(),
        status: CheckStatus::Pass,
        coverage_bps: 10_000,
        strict: false,
    }
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

mod phases;
mod verification;
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
