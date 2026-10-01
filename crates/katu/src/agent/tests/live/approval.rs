//! E07-T05 — aprovação interativa no loop: capacidade mínima, assinatura e não-herança.

use std::path::Path;

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Message};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::super::{options, request, root};
use super::Recorder;
use crate::agent::{Activity, ActivitySink, Approval, ApprovalPrompt, Ports, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Escreve a config de teste com a chave MAC (D3).
fn write_mac_config(workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = workspace.join(".katu");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("katu.toml"), "audit.mac_key = \"test-key\"\n")?;
    Ok(())
}

/// Aprovador de teste: aprova só quando o âmbito contém `approve_containing`.
#[derive(Default)]
struct Approver {
    scopes: Vec<String>,
    approve_containing: String,
}

impl ActivitySink for Approver {
    fn activity(&mut self, _activity: Activity<'_>) {}

    fn approve(&mut self, prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        self.scopes.push(prompt.request.scope.clone());
        prompt
            .request
            .scope
            .contains(&self.approve_containing)
            .then(|| Approval {
                reason: "necessário para o teste".to_string(),
                granted_by: "ana".to_string(),
            })
    }
}

/// Pedido de leitura absoluta de `path`.
fn read_call(call: &str, path: &Path) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(call),
        name: "read".to_string(),
        arguments: json!({"path": path.display().to_string()}),
    }
}

/// Resultados de tool no log, por ordem.
fn outcomes(runtime: &Runtime<'_>) -> Result<Vec<ToolOutcome>, Box<dyn std::error::Error>> {
    Ok(runtime
        .messages()?
        .into_iter()
        .filter_map(|message| match message {
            Message::ToolResult { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect())
}

#[test]
fn approval_unlocks_an_outside_read_and_is_logged() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = root("approval-ws")?;
    write_mac_config(&workspace)?;
    let target_dir = root("approval-target")?;
    let target = target_dir.join("dados.txt");
    std::fs::write(&target, "conteúdo")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &workspace, "ler fora")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call("o1", &target)],
                stop: StopReason::ToolCalls,
            },
            Turn::text("ok"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let mut approver = Approver {
        approve_containing: "dados.txt".to_string(),
        ..Approver::default()
    };
    run_turn_with(
        &mut runtime,
        request(&provider, ports, "ler fora", &options(4)),
        &mut approver,
    )?;
    assert_eq!(approver.scopes.len(), 1, "uma aprovação pedida");
    let outcomes = outcomes(&runtime)?;
    assert!(
        outcomes.iter().any(|outcome| matches!(outcome, ToolOutcome::Unavailable { control, .. } if control.as_str() == "approval")),
        "a recusa fica no log antes da aprovação"
    );
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, ToolOutcome::Ok)),
        "a releitura aprovada sucede"
    );
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&workspace)?;
    std::fs::remove_dir_all(&target_dir)?;
    Ok(())
}

#[test]
fn an_approval_is_not_inherited_by_a_different_path() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = root("approval-scope-ws")?;
    write_mac_config(&workspace)?;
    let dir = root("approval-scope-target")?;
    let first = dir.join("dados.txt");
    let second = dir.join("outro.txt");
    std::fs::write(&first, "a")?;
    std::fs::write(&second, "b")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &workspace, "ler dois")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call("a1", &first), read_call("a2", &second)],
                stop: StopReason::ToolCalls,
            },
            Turn::text("ok"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let mut approver = Approver {
        approve_containing: "dados.txt".to_string(),
        ..Approver::default()
    };
    run_turn_with(
        &mut runtime,
        request(&provider, ports, "ler dois", &options(4)),
        &mut approver,
    )?;
    assert_eq!(
        approver.scopes.len(),
        2,
        "o caminho diferente volta a pedir aprovação (não herdada)"
    );
    let outcomes = outcomes(&runtime)?;
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, ToolOutcome::Ok))
    );
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, ToolOutcome::Unavailable { .. }))
    );
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&workspace)?;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn an_outside_read_without_approval_stays_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = root("approval-fail-closed-ws")?;
    let dir = root("approval-fail-closed-target")?;
    let target = dir.join("dados.txt");
    std::fs::write(&target, "x")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &workspace, "sem aprovação")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call("n1", &target)],
                stop: StopReason::ToolCalls,
            },
            Turn::text("ok"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let mut recorder = Recorder::default();
    run_turn_with(
        &mut runtime,
        request(&provider, ports, "sem aprovação", &options(4)),
        &mut recorder,
    )?;
    assert_eq!(recorder.unavailable, ["approval".to_string()]);
    assert!(
        !outcomes(&runtime)?
            .iter()
            .any(|outcome| matches!(outcome, ToolOutcome::Ok)),
        "sem aprovação nada corre além da leitura recusada"
    );
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&workspace)?;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
