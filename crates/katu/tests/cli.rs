//! Testes de ponta a ponta da CLI (E01-T04).

use std::io;
use std::process::Command;

/// `katu --json version` devolve envelope válido com `success: true`.
#[test]
fn json_envelope_on_version() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["--json", "version"])
        .output()?;
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"success\":true"), "stdout: {text}");
    assert!(text.contains("\"command\":\"version\""), "stdout: {text}");
    Ok(())
}

/// `katu version` (humano) escreve dados em `stdout` e nada em `stderr`.
#[test]
fn human_version_goes_to_stdout() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .arg("version")
        .output()?;
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    Ok(())
}

/// Sem subcomando o `clap` mostra ajuda e sai com código de uso (2).
#[test]
fn missing_subcommand_is_usage_error() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu")).output()?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

/// `katu --json sessions` lista as sessões do projeto (possivelmente vazio) sem erro.
#[test]
fn sessions_lists_without_error() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["--json", "sessions"])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"command\":\"sessions\""), "stdout: {text}");
    Ok(())
}

/// `--resume` com um id desconhecido **recusa** (fail-closed), sem inventar sessão.
#[test]
fn resume_unknown_session_fails_closed() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["--json", "run", "--resume", "s_0000000000000000", "olá"])
        .output()?;
    assert!(!output.status.success(), "devia falhar");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"success\":false"), "stdout: {text}");
    Ok(())
}
