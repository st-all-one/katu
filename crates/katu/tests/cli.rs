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
