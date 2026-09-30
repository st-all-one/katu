//! Testes do loop de turnos (E12-T05): provider fake + tools reais, **sem rede**.
//!
//! Provam a ordem §42 (logar → política → efeito), que o resultado volta ao modelo e que o turno
//! termina de forma determinística; a memória real in-process já é coberta pelo runtime.

use std::path::PathBuf;

use katu_core::kernel::{CallId, Message};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ModelSpec, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{Ports, TurnOptions, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Raiz temporária única por teste.
fn root(label: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("katu-agent-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Ferramenta de escrita que o modelo pede no guião.
fn write_call() -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new("c1"),
        name: "write".to_string(),
        arguments: json!({"path": "new.txt", "content": "olá"}),
    }
}

/// Opções mínimas de turno.
fn options(max_steps: u32) -> TurnOptions {
    TurnOptions {
        model: ModelSpec::new("fake"),
        system: None,
        max_tokens: 128,
        temperature: 0.0,
        max_steps,
    }
}

#[test]
fn loop_executes_a_tool_then_stops() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("tool-then-stop")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "escreve um ficheiro")?;

    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![write_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("feito"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        &provider,
        &ports,
        "escreve um ficheiro",
        &options(4),
    )?;
    assert_eq!(report.steps, 2);
    assert_eq!(report.calls, 1);
    assert_eq!(report.text, "feito");
    assert!(root.join("new.txt").exists(), "a tool escreveu o ficheiro");

    // §42: o pedido e o resultado ficam no log, pela ordem.
    let messages = runtime.messages()?;
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::ToolCall { .. }))
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::ToolResult { .. }))
    );
    assert!(matches!(messages.last(), Some(Message::Assistant { .. })));
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn loop_refuses_to_spin_past_the_step_budget() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("budget")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "insiste")?;

    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![write_call()],
                stop: StopReason::ToolCalls,
            },
            Turn {
                events: vec![write_call()],
                stop: StopReason::ToolCalls,
            },
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let result = run_turn(&mut runtime, &provider, &ports, "insiste", &options(1));
    assert!(matches!(
        result,
        Err(super::AgentError::TooManySteps { .. })
    ));

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
