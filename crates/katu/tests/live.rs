//! Testes **live** do `run` (E20-T04), gated por `KATU_LLAMA_URL`.
//!
//! Sem a variável (base `OpenAI`-compatible, ex.: `http://127.0.0.1:8081/v1`), os testes são
//! **no-op** (skip). Com ela, correm o ciclo completo: `run` cria a sessão e devolve o id, e
//! `run --resume <id>` continua a **mesma** sessão. Não correm em CI (sem `llama-server`).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Base do `llama-server` (`/v1`); ausente ou vazia = skip.
fn base() -> Option<String> {
    std::env::var_os("KATU_LLAMA_URL")
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
}

/// Cria um diretório temporário isolado por teste.
fn temp_dir(tag: &str) -> Result<PathBuf, io::Error> {
    let dir = std::env::temp_dir().join(format!("katu-live-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Corre o binário com `cwd`/`HOME` isolados.
fn run_in(dir: &Path, args: &[&str]) -> Result<std::process::Output, io::Error> {
    Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .output()
}

/// Extrai o campo `"session":"<id>"` de um envelope JSON.
fn session_of(text: &str) -> Option<String> {
    let marker = "\"session\":\"";
    let start = text.find(marker)?.saturating_add(marker.len());
    let rest = text.get(start..)?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_owned)
}

/// O id devolvido pelo `run` é aceito por `--resume` na execução seguinte (T04).
#[test]
fn run_returns_a_resumable_session_id() -> Result<(), io::Error> {
    let Some(base) = base() else {
        return Ok(());
    };
    let dir = temp_dir("resume")?;
    let first = run_in(
        &dir,
        &[
            "run",
            "diga apenas: oi",
            "--provider",
            "llama",
            "--base",
            &base,
            "--json",
        ],
    )?;
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_text = String::from_utf8_lossy(&first.stdout);
    let session = session_of(&first_text).unwrap_or_default();
    assert!(!session.is_empty(), "envelope sem `session`: {first_text}");

    let resumed = run_in(
        &dir,
        &[
            "run",
            "continua",
            "--resume",
            &session,
            "--provider",
            "llama",
            "--base",
            &base,
            "--json",
        ],
    )?;
    assert!(
        resumed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let resumed_text = String::from_utf8_lossy(&resumed.stdout);
    assert_eq!(
        session_of(&resumed_text).as_deref(),
        Some(session.as_str()),
        "a retomada mudou de sessão: {resumed_text}"
    );

    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// Um id de sessão desconhecido recusa com `invalid_input` (exit 2).
#[test]
fn resume_with_unknown_id_fails_closed() -> Result<(), io::Error> {
    let Some(base) = base() else {
        return Ok(());
    };
    let dir = temp_dir("resume-unknown")?;
    let output = run_in(
        &dir,
        &[
            "run",
            "oi",
            "--resume",
            "s_0000000000000000",
            "--provider",
            "llama",
            "--base",
            &base,
            "--json",
        ],
    )?;
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}
