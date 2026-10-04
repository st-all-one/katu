use super::{Budget, BudgetCap, BudgetGate, BudgetRefusal, Charge, charge_for};
use crate::kernel::Visibility;
use crate::kernel::event::{CallId, Event};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

fn write_tool() -> Result<ToolUse, katu_policy::PolicyError> {
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
fn unlimited_gate_never_refuses() {
    let gate = BudgetGate::unlimited();
    assert!(gate.check(Charge::Turn).is_ok());
    assert!(gate.check(Charge::Tokens(u64::MAX)).is_ok());
    assert_eq!(gate.usage(), Budget::ZERO);
}

#[test]
fn cap_is_reached_then_refused_without_mutation() {
    let cap = BudgetCap {
        turns: Some(2),
        ..BudgetCap::NONE
    };
    let mut gate = BudgetGate::new(cap);
    gate.commit(Charge::Turn);
    assert!(gate.check(Charge::Turn).is_ok());
    gate.commit(Charge::Turn);
    assert_eq!(gate.usage().turns, 2);

    let before = gate.usage();
    let refusal = gate.check(Charge::Turn);
    assert_eq!(refusal, Err(BudgetRefusal::Turns { used: 3, cap: 2 }));
    assert_eq!(gate.usage(), before, "a recusa nunca corta o uso (§29)");
}

#[test]
fn each_axis_has_its_own_refusal() {
    let cap = BudgetCap {
        turns: Some(0),
        tool_calls: Some(0),
        tokens: Some(0),
        wall_clock_ms: Some(0),
    };
    let gate = BudgetGate::new(cap);
    assert_eq!(
        gate.check(Charge::Turn),
        Err(BudgetRefusal::Turns { used: 1, cap: 0 })
    );

    let mut gate = BudgetGate::new(BudgetCap {
        tool_calls: Some(0),
        ..BudgetCap::NONE
    });
    gate.commit(Charge::ToolCall);
    assert_eq!(
        gate.check(Charge::ToolCall),
        Err(BudgetRefusal::ToolCalls { used: 2, cap: 0 })
    );

    let gate = BudgetGate::new(BudgetCap {
        tokens: Some(10),
        ..BudgetCap::NONE
    });
    assert_eq!(
        gate.check(Charge::Tokens(11)),
        Err(BudgetRefusal::Tokens { used: 11, cap: 10 })
    );

    let gate = BudgetGate::new(BudgetCap {
        wall_clock_ms: Some(5),
        ..BudgetCap::NONE
    });
    assert_eq!(
        gate.check(Charge::WallClock(6)),
        Err(BudgetRefusal::WallClock { used: 6, cap: 5 })
    );
}

#[test]
fn events_reconstruct_turn_and_call_usage() -> Result<(), Box<dyn std::error::Error>> {
    let events = vec![
        Event::TurnStart { turn: 1 },
        Event::UserMessage {
            text: "oi".into(),
            visibility: Visibility::User,
        },
        Event::ToolCall {
            call: CallId::new("c1"),
            tool: write_tool()?,
        },
    ];
    let usage = Budget::from_events(&events);
    assert_eq!(usage.turns, 1);
    assert_eq!(usage.tool_calls, 1);
    assert!(events.get(1).and_then(charge_for).is_none());
    Ok(())
}

#[test]
fn resume_keeps_caps_and_usage() {
    let cap = BudgetCap {
        turns: Some(3),
        ..BudgetCap::NONE
    };
    let gate = BudgetGate::resume(
        cap,
        Budget {
            turns: 1,
            ..Budget::ZERO
        },
    );
    assert_eq!(gate.cap(), cap);
    assert!(gate.check(Charge::Turn).is_ok());
}
