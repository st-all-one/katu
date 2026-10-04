//! Testes do fim por tool **terminal** (`Q1/PI_GAINS`): o verbo corta o loop sem passo extra.
//!
//! O catálogo de produção não tem um verbo terminal, pelo que o seam de teste `finish`
//! (`router::tools`, `cfg(test)`) exercita o mecanismo ponta-a-ponta sem alargar a superfície.

use katu_core::kernel::CallId;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{Ports, options, request, root, write_call};
use crate::agent::{Termination, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Tool terminal de teste (seam `finish`).
fn finish_call(call: &str) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(call),
        name: "finish".to_string(),
        arguments: json!({}),
    }
}

/// Um passo cujo **único** resultado é terminal fecha o turno sem pedir outro passo ao provider.
#[test]
fn a_terminal_tool_ends_the_turn_without_another_step() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("terminate")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "termina")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![finish_call("f1")],
                stop: StopReason::ToolCalls,
            },
            // Se o loop continuasse, consumiria este turno.
            Turn::text("não devia ser pedido"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "termina", &options(4)),
    )?;
    assert_eq!(report.termination, Termination::Terminal);
    assert_eq!(report.steps, 1, "um fim terminal não gasta outro passo");
    assert!(report.text.is_empty(), "{}", report.text);
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Um passo **misto** (terminal + não-terminal) continua: a regra é "todas as calls do passo".
#[test]
fn a_mixed_step_continues() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("terminate-misto")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "misto")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![finish_call("f1"), write_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("feito"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "misto", &options(4)),
    )?;
    assert_eq!(report.termination, Termination::Natural);
    assert_eq!(report.steps, 2, "uma call não-terminal obriga a novo passo");
    assert_eq!(report.text, "feito");
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
