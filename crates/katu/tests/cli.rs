//! Testes de ponta a ponta da CLI (E01-T04, E20).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Cria um diretório temporário isolado por teste.
fn temp_dir(tag: &str) -> Result<PathBuf, io::Error> {
    let base = std::env::temp_dir().join(format!("katu-it-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Corre o binário com `cwd`/`HOME` isolados num diretório temporário.
fn run_in(dir: &Path, args: &[&str]) -> Result<std::process::Output, io::Error> {
    Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .output()
}

/// `katu prime --json` devolve envelope válido com `success: true`.
#[test]
fn json_envelope_on_prime() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["prime", "--json"])
        .output()?;
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"success\":true"), "stdout: {text}");
    assert!(text.contains("\"command\":\"prime\""), "stdout: {text}");
    Ok(())
}

/// `katu prime` (humano) escreve dados em `stdout` e nada em `stderr`.
#[test]
fn human_prime_goes_to_stdout() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .arg("prime")
        .output()?;
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    Ok(())
}

/// `katu` sem subcomando não abre a TUI sem TTY: falha fechado.
#[test]
fn no_subcommand_without_tty_fails_closed() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu")).output()?;
    assert!(!output.status.success(), "devia falhar sem TTY");
    assert!(!output.stderr.is_empty());
    Ok(())
}

/// `katu memo sessions --json` lista as sessões do projeto sem erro.
#[test]
fn memo_sessions_lists_without_error() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["memo", "sessions", "--json"])
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
    let dir = temp_dir("resume")?;
    let output = run_in(
        &dir,
        &["run", "--json", "--resume", "s_0000000000000000", "olá"],
    )?;
    assert!(!output.status.success(), "devia falhar");
    assert_eq!(
        output.status.code(),
        Some(2),
        "id desconhecido = invalid_input"
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"success\":false"), "stdout: {text}");
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// Verbos antigos deixam de existir no topo (sem retrocompatibilidade).
#[test]
fn old_top_level_verbs_are_gone() -> Result<(), io::Error> {
    for verb in [
        "version", "doctor", "memory", "sessions", "recall", "remember",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_katu"))
            .arg(verb)
            .output()?;
        assert_eq!(output.status.code(), Some(2), "verbo `{verb}` devia sair 2");
    }
    Ok(())
}

/// `config set` grava no projeto e `config get` lê a config efetiva (T09).
#[test]
fn config_set_get_round_trip() -> Result<(), io::Error> {
    let dir = temp_dir("config")?;
    let set = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["config", "set", "provider", "llama"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .output()?;
    assert!(
        set.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let get = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["config", "get", "provider", "--json"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .output()?;
    assert!(get.status.success());
    let text = String::from_utf8_lossy(&get.stdout);
    assert!(text.contains("\"value\":\"llama\""), "stdout: {text}");
    assert!(
        dir.join(".katu/katu.toml").is_file(),
        "config do projeto ausente"
    );
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// Chave de configuração desconhecida é recusada (T09).
#[test]
fn unknown_config_key_is_rejected() -> Result<(), io::Error> {
    let dir = temp_dir("config-key")?;
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["config", "set", "nope", "x"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `--params` e flags explícitas são exclusivos (T08).
#[test]
fn params_and_flags_are_exclusive() -> Result<(), io::Error> {
    let dir = temp_dir("params-xor")?;
    let output = run_in(
        &dir,
        &["run", "--params", "{\"body\":\"x\"}", "--provider", "llama"],
    )?;
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `--params` recusa campos desconhecidos (fail-closed, T08).
#[test]
fn params_unknown_field_is_rejected() -> Result<(), io::Error> {
    let dir = temp_dir("params-field")?;
    let output = run_in(&dir, &["run", "--params", "{\"nope\":1}"])?;
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `katu --init` cria o layout `.katu/` e o snapshot da config (T19/T18).
#[test]
fn init_creates_project_layout() -> Result<(), io::Error> {
    let dir = temp_dir("init")?;
    let first = run_in(&dir, &["--init"])?;
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    for sub in ["audit", "knowledge", "guardrails", "trash", "log", "plan"] {
        assert!(dir.join(".katu").join(sub).is_dir(), "falta .katu/{sub}");
    }
    assert!(
        dir.join(".katu/katu.toml").is_file(),
        "falta snapshot da config"
    );
    let second = run_in(&dir, &["--init"])?;
    assert!(
        second.status.success(),
        "segunda execução devia ser idempotente"
    );
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `--init --git-excluded` marca `.katu/` no `.git/info/exclude` (T19).
#[test]
fn init_git_excluded_adds_exclude() -> Result<(), io::Error> {
    let dir = temp_dir("init-git")?;
    std::fs::create_dir_all(dir.join(".git/info"))?;
    let output = run_in(&dir, &["--init", "--git-excluded"])?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exclude = std::fs::read_to_string(dir.join(".git/info/exclude"))?;
    assert!(exclude.contains(".katu/"), "exclude: {exclude}");
    let gitignore = std::fs::read_to_string(dir.join(".gitignore"))?;
    assert!(gitignore.contains(".katu/audit/"), "gitignore: {gitignore}");
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `memo doctor --fix` faz o bootstrap do projeto (T07).
#[test]
fn doctor_fix_bootstraps_project() -> Result<(), io::Error> {
    let dir = temp_dir("doctor-fix")?;
    let output = run_in(&dir, &["memo", "doctor", "--fix"])?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join(".katu/katu.toml").is_file());
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `katu <grupo> prime` é idêntico a `katu prime --group <grupo>`.
#[test]
fn group_prime_matches_global_prime() -> Result<(), io::Error> {
    for group in ["memo", "config"] {
        let a = Command::new(env!("CARGO_BIN_EXE_katu"))
            .args([group, "prime", "--json"])
            .output()?;
        let b = Command::new(env!("CARGO_BIN_EXE_katu"))
            .args(["prime", "--group", group, "--json"])
            .output()?;
        assert!(
            a.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&a.stderr)
        );
        assert!(b.status.success());
        assert_eq!(a.stdout, b.stdout, "grupo `{group}` divergiu");
    }
    Ok(())
}

/// `memo drain --digest` drena sem servidor de embeddings (fail-closed, sem pânico).
#[test]
fn memo_drain_digest_is_fail_closed() -> Result<(), io::Error> {
    let dir = temp_dir("drain-digest")?;
    let output = run_in(&dir, &["memo", "drain", "--digest", "--json"])?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("\"command\":\"memo.drain\""),
        "stdout: {text}"
    );
    Ok(())
}

/// `memo drain --force` exige `--digest` (exit 2).
#[test]
fn memo_drain_force_requires_digest() -> Result<(), io::Error> {
    let dir = temp_dir("drain-force")?;
    let output = run_in(&dir, &["memo", "drain", "--force"])?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

/// As ações do worker exigem `--watch-service` (exit 2).
#[test]
fn memo_drain_watch_actions_require_flag() -> Result<(), io::Error> {
    let dir = temp_dir("drain-watch")?;
    let output = run_in(&dir, &["memo", "drain", "--install"])?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

/// `prime --params` e flags explícitas são exclusivos (T08).
#[test]
fn prime_params_and_flag_are_exclusive() -> Result<(), io::Error> {
    let output = Command::new(env!("CARGO_BIN_EXE_katu"))
        .args(["prime", "--params", "{}", "--long"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

/// `--init --force` preserva o conhecimento personalizado e refaz o resto (T19).
#[test]
fn init_force_preserves_knowledge_and_resets_rest() -> Result<(), io::Error> {
    let dir = temp_dir("init-force")?;
    assert!(run_in(&dir, &["--init"])?.status.success());
    std::fs::write(dir.join(".katu/knowledge/nota.md"), "nota")?;
    std::fs::write(dir.join(".katu/trash/velho.txt"), "lixo")?;
    let forced = run_in(&dir, &["--init", "--force"])?;
    assert!(
        forced.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert!(
        dir.join(".katu/knowledge/nota.md").is_file(),
        "nota devia ser preservada"
    );
    assert!(
        !dir.join(".katu/trash/velho.txt").exists(),
        "trash devia ser refeito"
    );
    assert!(
        dir.join(".katu/katu.toml").is_file(),
        "config devia ser recriada"
    );
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}
