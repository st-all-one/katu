//! Testes unitários da borda CLI (E20).

use clap::Parser;

use super::{Cli, LogLevel};

/// O nível de log é global e o default é `quiet`.
#[test]
fn log_level_defaults_to_quiet() {
    let cli = Cli::parse_from(["katu", "prime"]);
    assert_eq!(cli.log_level, LogLevel::Quiet);
    assert!(!cli.json());
}

/// `--json` é por comando (não global).
#[test]
fn json_flag_is_per_command() {
    let cli = Cli::parse_from(["katu", "prime", "--json"]);
    assert!(cli.json());
}

/// A TUI não tem `--json` (não produz dados).
#[test]
fn tui_has_no_json_flag() {
    let result = Cli::try_parse_from(["katu", "tui", "--json"]);
    assert!(result.is_err(), "tui não devia aceitar --json");
}

/// `--init` aceita um modo de git e só um.
#[test]
fn init_takes_one_git_mode() {
    let cli = Cli::parse_from(["katu", "--init", "--git-excluded"]);
    assert!(cli.init && cli.git_excluded && !cli.git_tracked);
    let both = Cli::try_parse_from(["katu", "--init", "--git-excluded", "--git-tracked"]);
    assert!(both.is_err(), "os dois modos deviam colidir");
}

/// `--git-excluded` sem `--init` é erro de uso.
#[test]
fn git_mode_requires_init() {
    let result = Cli::try_parse_from(["katu", "--git-excluded"]);
    assert!(result.is_err(), "--git-excluded devia exigir --init");
}

/// `--force` exige `--init` (T19).
#[test]
fn force_requires_init() {
    assert!(Cli::try_parse_from(["katu", "--init", "--force"]).is_ok());
    assert!(Cli::try_parse_from(["katu", "--force"]).is_err());
}
