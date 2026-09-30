use super::{
    CheckStatus, Override, VERIFICATION_SCHEMA_VERSION, VerificationInput, VerificationReport,
    VerifyError, append_override, overrides_path, report_path, save, verify,
};
use crate::feedback::CommandRecord;
use crate::plan::ScopeContract;
use crate::ports::{Fs, MemFs};
use std::path::Path;

fn scope() -> ScopeContract {
    ScopeContract::new(
        vec!["src/**".to_string()],
        vec!["secrets/**".to_string()],
        vec!["testes passam".to_string()],
        "reverter",
    )
}

fn paths(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// Desfecho de um comando nos testes (sem booleano como parâmetro).
#[derive(Clone, Copy)]
enum Outcome {
    Exit(i32),
    Ambiguous,
    Timeout,
}

fn command(outcome: Outcome) -> CommandRecord {
    let (exit_code, timed_out) = match outcome {
        Outcome::Exit(code) => (Some(code), false),
        Outcome::Ambiguous => (None, false),
        Outcome::Timeout => (None, true),
    };
    CommandRecord {
        id: "c1".to_string(),
        argv: vec!["cargo".to_string(), "test".to_string()],
        cwd: "/work".to_string(),
        exit_code,
        signal: None,
        timed_out,
        duration_ms: 1,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        parent_command_id: None,
    }
}

fn check_status(report: &VerificationReport, id: &str) -> Option<CheckStatus> {
    report
        .checks
        .iter()
        .find(|check| check.id == id)
        .map(|check| check.status)
}

#[test]
fn same_input_yields_the_same_report() {
    let scope = scope();
    let files = paths(&["src/main.rs"]);
    let commands = [command(Outcome::Exit(0))];
    let input = VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &commands,
        coverage_floor_bps: 9_000,
        strict: false,
    };
    assert_eq!(verify(&input), verify(&input));
    assert_eq!(verify(&input).schema_version, VERIFICATION_SCHEMA_VERSION);
}

#[test]
fn forbidden_file_blocks() {
    let scope = scope();
    let files = paths(&["secrets/token"]);
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &[],
        coverage_floor_bps: 0,
        strict: false,
    });
    assert!(report.is_blocked());
    assert_eq!(
        check_status(&report, "scope.forbidden"),
        Some(CheckStatus::Block)
    );
}

#[test]
fn outside_scope_warns_and_strict_promotes_to_block() {
    let scope = scope();
    let files = paths(&["docs/readme.md"]);
    let lax = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &[],
        coverage_floor_bps: 0,
        strict: false,
    });
    assert_eq!(lax.status, CheckStatus::Warn);
    let strict = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &[],
        coverage_floor_bps: 0,
        strict: true,
    });
    assert!(strict.is_blocked(), "--strict promove o aviso a muro");
}

#[test]
fn ambiguous_or_timed_out_commands_block() {
    let scope = scope();
    let files = paths(&["src/main.rs"]);
    let ambiguous = [command(Outcome::Ambiguous)];
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &ambiguous,
        coverage_floor_bps: 0,
        strict: false,
    });
    assert_eq!(
        check_status(&report, "feedback.ambiguous"),
        Some(CheckStatus::Block)
    );
    let timed_out = [command(Outcome::Timeout)];
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &timed_out,
        coverage_floor_bps: 0,
        strict: false,
    });
    assert_eq!(
        check_status(&report, "feedback.timeout"),
        Some(CheckStatus::Block)
    );
}

#[test]
fn nonzero_exit_warns() {
    let scope = scope();
    let files = paths(&["src/main.rs"]);
    let failed = [command(Outcome::Exit(1))];
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &failed,
        coverage_floor_bps: 0,
        strict: false,
    });
    assert_eq!(
        check_status(&report, "feedback.exit"),
        Some(CheckStatus::Warn)
    );
}

#[test]
fn coverage_floor_is_enforced() {
    let scope = scope();
    let files = paths(&["src/a.rs", "docs/b.md"]);
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &[],
        coverage_floor_bps: 9_000,
        strict: false,
    });
    assert_eq!(report.coverage_bps, 5_000);
    assert_eq!(check_status(&report, "coverage"), Some(CheckStatus::Warn));
}

#[test]
fn override_requires_a_signature() {
    assert!(matches!(
        Override::new("scope.forbidden", "", "ana", 1),
        Err(VerifyError::MissingField("reason"))
    ));
    assert!(matches!(
        Override::new("scope.forbidden", "risco aceite", "   ", 1),
        Err(VerifyError::MissingField("overridden_by"))
    ));
    assert!(Override::new("scope.forbidden", "risco aceite", "ana", 1).is_ok());
}

#[test]
fn override_is_registered_append_only() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    append_override(
        &fs,
        dir,
        &Override::new("scope.forbidden", "risco aceite", "ana", 1)?,
    )?;
    append_override(
        &fs,
        dir,
        &Override::new("scope.forbidden", "segunda vez", "rui", 2)?,
    )?;
    let text = String::from_utf8(fs.read(&overrides_path(dir))?)?;
    assert_eq!(text.lines().count(), 2, "append-only, sem sobrepor");
    let first = text.lines().next().ok_or("sem linha")?;
    let parsed: Override = serde_json::from_str(first)?;
    assert_eq!(parsed.overridden_by, "ana");
    Ok(())
}

#[test]
fn report_is_saved_atomically() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let scope = scope();
    let files = paths(&["src/main.rs"]);
    let report = verify(&VerificationInput {
        changed_files: &files,
        scope: &scope,
        commands: &[],
        coverage_floor_bps: 0,
        strict: false,
    });
    save(&fs, dir, &report)?;
    let parsed: VerificationReport = serde_json::from_slice(&fs.read(&report_path(dir))?)?;
    assert_eq!(parsed, report);
    Ok(())
}
