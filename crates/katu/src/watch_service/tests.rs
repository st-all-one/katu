//! Testes do worker de auto-drain (E20-T20): portas falsas, sem tocar no sistema.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use katu_core::ports::{FakeEnv, MemProcess, ProcessError};
use serde_json::Value;

use super::{Action, Context, Paths, dispatch};

/// Diretório temporário único por teste.
fn temp(label: &str) -> Result<PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join(format!("katu-watch-{}-{label}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Caminhos do worker dentro de um diretório temporário.
fn paths(temp: &Path) -> Paths {
    Paths {
        base: temp.join("data/katu"),
        systemd: temp.join("config/systemd/user"),
    }
}

/// Lê as linhas de um ficheiro (vazio se ausente).
fn read_lines(path: &Path) -> Result<Vec<String>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.lines().map(str::to_string).collect()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// `--subscribe` é idempotente; `--unsubscribe` remove o projeto.
#[test]
fn subscribe_is_idempotent_and_unsubscribe_removes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp("subscribe")?;
    let paths = paths(&temp);
    let env = FakeEnv::new().with_var("HOME", ".");
    let process = MemProcess::ok("");
    let project = temp.join("proj");
    std::fs::create_dir_all(&project)?;
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };

    dispatch(Action::Subscribe, &context)?;
    dispatch(Action::Subscribe, &context)?;
    assert_eq!(
        read_lines(&paths.watched())?,
        vec![project.display().to_string()]
    );

    dispatch(Action::Unsubscribe, &context)?;
    assert!(read_lines(&paths.watched())?.is_empty());
    std::fs::remove_dir_all(&temp).ok();
    Ok(())
}

/// `--install` materializa script + unidades e ativa o timer.
#[test]
fn install_materializes_and_enables() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp("install")?;
    let paths = paths(&temp);
    let env = FakeEnv::new().with_var("HOME", ".");
    let process = MemProcess::ok("");
    let project = temp.join("proj");
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };

    let data = dispatch(Action::Install, &context)?;
    assert_eq!(data.get("installed").and_then(Value::as_bool), Some(true));
    assert!(paths.script().is_file(), "script materializado");
    assert!(paths.service().is_file(), "unidade do serviço");
    assert!(paths.timer().is_file(), "unidade do timer");

    let runs = process.runs();
    assert_eq!(runs.len(), 2, "daemon-reload + enable");
    assert!(
        runs.first()
            .is_some_and(|run| run.argv.iter().any(|arg| arg == "daemon-reload"))
    );
    assert!(
        runs.get(1)
            .is_some_and(|run| run.argv.iter().any(|arg| arg == "enable"))
    );
    std::fs::remove_dir_all(&temp).ok();
    Ok(())
}

/// `--uninstall` desativa e remove script + unidades (mantém a lista).
#[test]
fn uninstall_disables_and_removes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp("uninstall")?;
    let paths = paths(&temp);
    let env = FakeEnv::new().with_var("HOME", ".");
    let process = MemProcess::ok("");
    let project = temp.join("proj");
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };

    dispatch(Action::Install, &context)?;
    let data = dispatch(Action::Uninstall, &context)?;
    assert_eq!(data.get("installed").and_then(Value::as_bool), Some(false));
    assert!(!paths.script().exists());
    assert!(!paths.service().exists());
    assert!(!paths.timer().exists());
    assert!(
        process
            .runs()
            .iter()
            .any(|run| run.argv.iter().any(|arg| arg == "disable"))
    );
    std::fs::remove_dir_all(&temp).ok();
    Ok(())
}

/// `--status` reporta a instalação sem tocar em nada.
#[test]
fn status_reports_installation() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp("status")?;
    let paths = paths(&temp);
    let env = FakeEnv::new().with_var("HOME", ".");
    let process = MemProcess::ok("");
    let project = temp.join("proj");
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };

    dispatch(Action::Install, &context)?;
    let data = dispatch(Action::Status, &context)?;
    assert_eq!(data.get("installed").and_then(Value::as_bool), Some(true));
    std::fs::remove_dir_all(&temp).ok();
    Ok(())
}

/// Sem `systemctl`, `--install` recusa (fail-closed) em vez de fingir sucesso.
#[test]
fn install_fails_closed_without_systemctl() -> Result<(), Box<dyn std::error::Error>> {
    let temp = temp("nosystemctl")?;
    let paths = paths(&temp);
    let env = FakeEnv::new().with_var("HOME", ".");
    let process = MemProcess::failing(ProcessError::NotFound);
    let project = temp.join("proj");
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };

    let Err(error) = dispatch(Action::Install, &context) else {
        return Err("install devia falhar sem systemctl".into());
    };
    assert!(error.to_string().contains("systemctl"), "erro: {error}");
    std::fs::remove_dir_all(&temp).ok();
    Ok(())
}
