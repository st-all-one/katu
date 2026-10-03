//! Testes do loop de turnos (E12-T05): provider fake + tools reais, **sem rede**.
//!
//! Provam a ordem §42 (logar → política → efeito), que o resultado volta ao modelo e que o turno
//! termina de forma determinística; a memória real in-process já é coberta pelo runtime.

use std::path::Path;

use katu_core::kernel::{CallId, Event, Message, read_records};
use katu_core::memory::{Memory, RecallReq};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{Ports, TurnOptions, TurnRequest, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

mod args;
mod calls;
mod context;
mod declared;
mod guard;
mod live;
mod steering;
mod stream;
mod support;
mod termination;
mod verify;
mod voi;
pub(crate) use support::{options, options_idle, read_call, request, root, write_call};

#[test]
fn loop_executes_a_tool_then_stops() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("tool-then-stop")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "escreve um ficheiro")?;

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![write_call()],
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
        request(&provider, ports, "escreve um ficheiro", &options(4)),
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

/// §18/G6 — **o payload da tool chega ao modelo**. Antes desta correção, o resultado projetado era
/// só o `ToolOutcome` (`{"Ok":null}`): o modelo ficava cego ao que a tool devolveu.
#[test]
fn the_tool_payload_reaches_the_model() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("delta-no-modelo")?;
    std::fs::write(root.join("nota.txt"), "conteudo secreto da nota\n")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê a nota")?;

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("li"),
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
        request(&provider, ports, "lê a nota", &options(4)),
    )?;
    assert_eq!(report.calls, 1);

    let messages = runtime.messages()?;
    let delta = messages
        .iter()
        .find_map(|message| match message {
            Message::ToolResult { delta, .. } => delta.clone(),
            _ => None,
        })
        .ok_or("o resultado tem de trazer o delta (§18/G6)")?;
    assert!(
        delta.contains("conteudo secreto da nota"),
        "o delta não traz o conteúdo lido: {delta}"
    );

    // O log é a fonte: o delta sobrevive ao replay (`Model-visible ⟺ logged`).
    let path = runtime.session().log_path().to_path_buf();
    drop(runtime);
    let logged = read_records(&fs, &path)?
        .into_iter()
        .find_map(|record| match record.event {
            Event::ToolResult { delta, .. } => delta,
            _ => None,
        });
    assert_eq!(logged.as_deref(), Some(delta.as_str()));

    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn loop_refuses_to_spin_past_the_step_budget() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("budget")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "insiste")?;

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
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
        request(&provider, ports, "insiste", &options(1)),
    )?;
    assert_eq!(
        report.termination,
        super::Termination::MaxSteps { steps: 1 }
    );

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Artefacto de plano válido na raiz do projeto (E09-T04).
fn write_plan(root: &Path) -> Result<(), std::io::Error> {
    std::fs::write(
        root.join("scope_contract.json"),
        r#"{"allowed_files":["src/**"],"forbidden_files":["**/secrets/**"],"acceptance_criteria":["testes passam"],"rollback_plan":"reverter"}"#,
    )?;
    std::fs::write(
        root.join("feature_list.json"),
        r#"[{"id":"F1","description":"fazer","status":"in_progress"}]"#,
    )
}

/// Pedido de plano do modelo (só `goal`/`next_action`; o contrato vem do artefacto).
pub(super) fn plan_call() -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new("p1"),
        name: "plan".to_string(),
        arguments: json!({"goal": "fazer", "next_action": "editar"}),
    }
}

#[test]
fn plan_artifact_is_recorded_through_the_loop() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("plan-artifact")?;
    write_plan(&root)?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "planear")?;
    assert!(
        runtime.plan().is_some(),
        "o artefacto foi carregado no arranque"
    );

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![plan_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("plano registado"),
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
        request(&provider, ports, "planear", &options(4)),
    )?;
    assert_eq!(report.calls, 1);
    assert!(
        runtime.session().state().plan.is_some(),
        "o plano ficou no estado pela ordem §42"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn two_turns_run_back_to_back_without_reopening() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("multi-turn")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "primeiro")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text("um"), Turn::text("dois")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let first = run_turn(
        &mut runtime,
        request(&provider, ports, "primeiro", &options(2)),
    )?;
    assert_eq!(first.text, "um");
    assert_eq!(runtime.session().state().turn, 1);
    assert!(!runtime.session().state().turn_open);

    let second = run_turn(
        &mut runtime,
        request(&provider, ports, "segundo", &options(2)),
    )?;
    assert_eq!(second.text, "dois");
    assert_eq!(runtime.session().state().turn, 2);

    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Ferramenta de escrita de memória que o modelo pede no guião.
pub(super) fn memory_record_call() -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new("m1"),
        name: "memory".to_string(),
        arguments: json!({"command": "record", "statement": "o projeto usa Rust"}),
    }
}

/// O agente usa o `Runtime::remember` para escrever na memória (recall prévio + gate de E05).
#[test]
fn agent_writes_memory_through_runtime_remember() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("memory-remember")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "escreve na memória")?;

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![memory_record_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("memória escrita"),
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
        request(&provider, ports, "escreve na memória", &options(4)),
    )?;
    assert_eq!(report.calls, 1);
    assert_eq!(report.text, "memória escrita");

    // Verifica que a nota foi escrita na memória
    let hits = runtime.memory.search(&RecallReq {
        query: "o projeto usa Rust".to_string(),
        limit: 5,
    })?;
    assert!(
        hits.iter()
            .any(|h| h.statement.contains("o projeto usa Rust")),
        "a nota foi escrita na memória"
    );

    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
