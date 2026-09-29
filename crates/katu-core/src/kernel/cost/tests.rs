use super::{
    CostCaps, CostCharge, CostGovernor, CostLayer, CostRefusal, Reenable, ReenableError,
    RollingWindowCap, VelocityCap, cost_charge_for,
};
use crate::kernel::budget::{Budget, BudgetCap, BudgetRefusal};
use crate::kernel::event::{CallId, Event};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use std::collections::BTreeMap;

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
fn per_tool_cap_fires_before_the_global_cap() {
    let caps = CostCaps {
        per_tool: BTreeMap::from([(ToolName::Write, 2)]),
        global: BudgetCap {
            tool_calls: Some(10),
            ..BudgetCap::NONE
        },
        ..CostCaps::default()
    };
    let mut governor = CostGovernor::new(caps, Budget::ZERO);
    let charge = CostCharge::tool_call(ToolName::Write);
    for _ in 0..2 {
        assert!(governor.check(&charge).is_ok());
        governor.commit(&charge);
    }
    let refusal = governor.check(&charge);
    assert_eq!(
        refusal.as_ref().err().map(CostRefusal::layer),
        Some(CostLayer::PerTool)
    );
    assert!(matches!(
        refusal,
        Err(CostRefusal::PerTool {
            tool: ToolName::Write,
            used: 3,
            cap: 2
        })
    ));
}

#[test]
fn global_cap_fires_when_there_is_no_per_tool_cap() {
    let caps = CostCaps {
        global: BudgetCap {
            tool_calls: Some(1),
            ..BudgetCap::NONE
        },
        ..CostCaps::default()
    };
    let mut governor = CostGovernor::new(caps, Budget::ZERO);
    let charge = CostCharge::tool_call(ToolName::Read);
    assert!(governor.check(&charge).is_ok());
    governor.commit(&charge);
    let refusal = governor.check(&charge);
    assert_eq!(
        refusal.as_ref().err().map(CostRefusal::layer),
        Some(CostLayer::Global)
    );
    assert!(matches!(
        refusal,
        Err(CostRefusal::Global(BudgetRefusal::ToolCalls {
            used: 2,
            cap: 1
        }))
    ));
}

#[test]
fn kill_switch_stops_everything_until_reenabled_separately()
-> Result<(), Box<dyn std::error::Error>> {
    let mut governor = CostGovernor::new(CostCaps::default(), Budget::ZERO);
    let charge = CostCharge::turn();
    assert!(governor.check(&charge).is_ok());
    governor.trip("loop patológico", 1_000);
    assert_eq!(
        governor.kill_switch().map(|kill| kill.reason.as_str()),
        Some("loop patológico")
    );
    assert!(matches!(
        governor.check(&charge),
        Err(CostRefusal::KillSwitch { .. })
    ));
    assert_eq!(Reenable::new("", "ana"), Err(ReenableError::EmptyReason));
    assert_eq!(
        Reenable::new("revisto", " "),
        Err(ReenableError::EmptyAuthorizedBy)
    );
    let grant = Reenable::new("revisto", "ana")?;
    governor.reenable(&grant);
    assert!(governor.kill_switch().is_none());
    assert!(governor.check(&charge).is_ok());
    Ok(())
}

#[test]
fn rolling_window_caps_the_rate() {
    let caps = CostCaps {
        rolling: Some(RollingWindowCap {
            max_calls: 2,
            window_ms: 1_000,
        }),
        ..CostCaps::default()
    };
    let mut governor = CostGovernor::new(caps, Budget::ZERO);
    governor.commit(&CostCharge::tool_call(ToolName::Read).at(0));
    governor.commit(&CostCharge::tool_call(ToolName::Read).at(100));
    let refusal = governor.check(&CostCharge::tool_call(ToolName::Read).at(200));
    assert_eq!(
        refusal.as_ref().err().map(CostRefusal::layer),
        Some(CostLayer::RollingWindow)
    );
    assert!(
        governor
            .check(&CostCharge::tool_call(ToolName::Read).at(2_000))
            .is_ok(),
        "fora da janela o teto já não conta"
    );
}

#[test]
fn financial_velocity_caps_the_spend() {
    let caps = CostCaps {
        velocity: Some(VelocityCap {
            max_micros_per_minute: 100,
        }),
        ..CostCaps::default()
    };
    let mut governor = CostGovernor::new(caps, Budget::ZERO);
    governor.commit(&CostCharge::tool_call(ToolName::Read).with_micros(60).at(0));
    let refusal = governor.check(
        &CostCharge::tool_call(ToolName::Read)
            .with_micros(50)
            .at(1_000),
    );
    assert_eq!(
        refusal.as_ref().err().map(CostRefusal::layer),
        Some(CostLayer::FinancialVelocity)
    );
    assert!(
        governor
            .check(
                &CostCharge::tool_call(ToolName::Read)
                    .with_micros(50)
                    .at(61_000)
            )
            .is_ok(),
        "passado um minuto a velocidade volta a zero"
    );
}

#[test]
fn refusal_does_not_mutate_usage() {
    let caps = CostCaps {
        global: BudgetCap {
            tool_calls: Some(1),
            ..BudgetCap::NONE
        },
        ..CostCaps::default()
    };
    let mut governor = CostGovernor::new(caps, Budget::ZERO);
    let charge = CostCharge::tool_call(ToolName::Read);
    governor.commit(&charge);
    let before = governor.global().usage();
    assert!(governor.check(&charge).is_err());
    assert_eq!(governor.global().usage(), before);
}

#[test]
fn from_events_reconstructs_per_tool_usage() -> Result<(), Box<dyn std::error::Error>> {
    let events = vec![
        Event::TurnStart { turn: 1 },
        Event::ToolCall {
            call: CallId::new("c1"),
            tool: write_tool()?,
        },
        Event::ToolCall {
            call: CallId::new("c2"),
            tool: write_tool()?,
        },
    ];
    let governor = CostGovernor::from_events(CostCaps::default(), &events);
    assert_eq!(governor.per_tool_used().get(&ToolName::Write), Some(&2));
    assert_eq!(governor.global().usage().tool_calls, 2);
    assert_eq!(governor.global().usage().turns, 1);
    Ok(())
}

#[test]
fn cost_charge_for_maps_events() -> Result<(), Box<dyn std::error::Error>> {
    assert!(cost_charge_for(&Event::TurnStart { turn: 1 }).is_some());
    let event = Event::ToolCall {
        call: CallId::new("c1"),
        tool: write_tool()?,
    };
    assert_eq!(
        cost_charge_for(&event).and_then(|charge| charge.tool),
        Some(ToolName::Write)
    );
    Ok(())
}
