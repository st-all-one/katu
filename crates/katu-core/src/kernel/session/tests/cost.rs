//! Cost governor ligado ao loop (E09-T06): teto por ferramenta antes do global e relógio.

use super::{CallContext, Probe, Session, SessionError, rules, use_write};
use crate::kernel::budget::BudgetCap;
use crate::kernel::cost::{CostCaps, CostRefusal, RollingWindowCap};
use crate::kernel::event::{CallId, Event};
use crate::ports::MemFs;
use katu_policy::ToolName;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicUsize;

#[test]
fn per_tool_cap_fires_before_the_global_cap_in_the_loop() -> Result<(), Box<dyn std::error::Error>>
{
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let caps = CostCaps {
        per_tool: BTreeMap::from([(ToolName::Write, 1)]),
        global: BudgetCap {
            tool_calls: Some(10),
            ..BudgetCap::NONE
        },
        ..CostCaps::default()
    };
    let mut session = Session::open_with_cost(&fs, dir, caps)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let rules = rules()?;
    let _dispatch = session.tool_call(
        CallId::new("c1"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules,
            now_millis: 0,
            tool: &probe,
        },
    )?;
    let refused = session.tool_call(
        CallId::new("c2"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules,
            now_millis: 0,
            tool: &probe,
        },
    );
    assert!(matches!(
        refused,
        Err(SessionError::Cost(CostRefusal::PerTool {
            tool: ToolName::Write,
            used: 2,
            cap: 1
        }))
    ));
    Ok(())
}

#[test]
fn rolling_window_uses_the_call_clock() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let caps = CostCaps {
        rolling: Some(RollingWindowCap {
            max_calls: 1,
            window_ms: 1_000,
        }),
        ..CostCaps::default()
    };
    let mut session = Session::open_with_cost(&fs, dir, caps)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    let probe = Probe {
        calls: AtomicUsize::new(0),
    };
    let rules = rules()?;
    let _dispatch = session.tool_call(
        CallId::new("c1"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules,
            now_millis: 0,
            tool: &probe,
        },
    )?;
    let refused = session.tool_call(
        CallId::new("c2"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules,
            now_millis: 100,
            tool: &probe,
        },
    );
    assert!(matches!(
        refused,
        Err(SessionError::Cost(CostRefusal::RollingWindow { .. }))
    ));
    let later = session.tool_call(
        CallId::new("c3"),
        &use_write("/work/x")?,
        CallContext {
            rules: &rules,
            now_millis: 2_000,
            tool: &probe,
        },
    );
    assert!(later.is_ok(), "fora da janela o teto já não dispara");
    Ok(())
}
