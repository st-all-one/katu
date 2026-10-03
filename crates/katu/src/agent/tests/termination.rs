//! Testes dos finais de turno (WL2): truncagem (L-Q2), vazio (L-Q3) e eco (L-Q4).

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Message};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use katu_providers::{FakeProvider, Turn};

use super::{Ports, options, request, root};
use crate::agent::{Termination, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Delta de tool longo o suficiente para a deteção de eco (L-Q4).
const DELTA: &str =
    "{\"kind\":\"read\",\"path\":\"nota.txt\",\"bytes\":42,\"lines\":3,\"sha\":\"abc123def456\"}";

/// Uso de `read` canónico.
fn read_use(root: &std::path::Path) -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical(root)?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

#[test]
fn a_length_finish_shows_a_visible_warning() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("length")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "responde")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn {
            events: vec![ProviderEvent::Text("resposta cortada a meio".to_string())],
            stop: StopReason::Length,
        }],
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
        request(&provider, ports, "responde", &options(4)),
    )?;
    assert_eq!(report.termination, Termination::Natural);
    assert!(report.text.contains("resposta cortada a meio"));
    assert!(
        report.text.contains("truncada pelo teto de tokens"),
        "{}",
        report.text
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn a_truncated_tool_call_is_closed_without_executing() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("truncada")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![ProviderEvent::ToolCallTruncated {
                    call: CallId::new("t1"),
                    name: "read".to_string(),
                }],
                stop: StopReason::Length,
            },
            Turn::text("reformulei a chamada"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(&mut runtime, request(&provider, ports, "lê", &options(4)))?;
    assert_eq!(report.termination, Termination::Natural);
    assert_eq!(report.text, "reformulei a chamada");

    // O log fecha a call como indisponível e ensina; o modelo **não** a executou.
    let messages = runtime.messages()?;
    let (outcome, delta) = messages
        .iter()
        .find_map(|message| match message {
            Message::ToolResult { outcome, delta, .. } => {
                Some((outcome.clone(), delta.clone().unwrap_or_default()))
            }
            _ => None,
        })
        .ok_or("esperava um ToolResult reconciliado")?;
    assert!(
        matches!(outcome, ToolOutcome::Unavailable { ref control, .. } if control.as_str() == "length"),
        "o resultado é `Unavailable{{length}}`: {outcome:?}"
    );
    assert!(delta.contains("truncada"), "{delta}");
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn an_empty_response_retries_then_explains() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("vazio")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "responde")?;
    let empty = || Turn {
        events: Vec::new(),
        stop: StopReason::EndTurn,
    };
    // Três respostas vazias: dois retries (MAX_EMPTY_RETRIES) e depois a mensagem final.
    let provider: std::sync::Arc<dyn Provider> =
        std::sync::Arc::new(FakeProvider::new("fake", vec![empty(), empty(), empty()]));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "responde", &options(6)),
    )?;
    assert_eq!(report.termination, Termination::Empty);
    assert_eq!(
        report.steps, 3,
        "dois retries + o passo que confirma o vazio"
    );
    assert!(
        report
            .text
            .contains("não consegui produzir uma resposta em texto"),
        "{}",
        report.text
    );
    // A mensagem final fica no log (visível e reproduzível).
    let messages = runtime.messages()?;
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::Assistant { text } if text.contains("não consegui produzir"))),
        "a mensagem final foi logada"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn an_echoed_tool_delta_is_nudged_and_retried() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("eco")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê")?;
    runtime.record_user("lê a nota")?;
    // Injeta um resultado de tool com um delta conhecido (o que o eco tem de reproduzir).
    let use_ = read_use(&root)?;
    runtime
        .session_mut()
        .begin_call(CallId::new("c1"), &use_, 1_000)?;
    runtime.session_mut().settle_call(
        CallId::new("c1"),
        ToolOutcome::Ok,
        Some(DELTA.to_string()),
    )?;

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text(DELTA), Turn::text("resposta própria")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(&mut runtime, request(&provider, ports, "lê", &options(4)))?;
    assert_eq!(report.termination, Termination::Natural);
    assert_eq!(report.text, "resposta própria", "o eco não é aceite (L-Q4)");
    assert!(!report.text.contains(DELTA));

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn a_persistent_echo_is_never_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("eco-persistente")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê")?;
    runtime.record_user("lê a nota")?;
    let use_ = read_use(&root)?;
    runtime
        .session_mut()
        .begin_call(CallId::new("c1"), &use_, 1_000)?;
    runtime.session_mut().settle_call(
        CallId::new("c1"),
        ToolOutcome::Ok,
        Some(DELTA.to_string()),
    )?;

    // Ecoa duas vezes: o retry esgota e o eco não pode ser aceite (G5).
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text(DELTA), Turn::text(DELTA)],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(&mut runtime, request(&provider, ports, "lê", &options(4)))?;
    assert_eq!(report.termination, Termination::Natural);
    assert!(!report.text.contains(DELTA), "o eco nunca é aceite (G5)");
    assert!(
        report.text.contains("substituída"),
        "fica uma nota visível no lugar do eco"
    );

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
