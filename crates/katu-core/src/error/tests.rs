use super::{Error, ErrorKind, ToolOutcome};
use katu_policy::{ControlId, Evidence, RuleId};
use std::io;

#[test]
fn exit_codes_are_distinct() {
    let kinds = [
        ErrorKind::NotFound,
        ErrorKind::InvalidInput,
        ErrorKind::Conflict,
        ErrorKind::Io,
        ErrorKind::Timeout,
        ErrorKind::Config,
        ErrorKind::Schema,
        ErrorKind::UnsafeBlocked,
        ErrorKind::Unavailable,
        ErrorKind::Internal,
    ];
    let mut codes: Vec<u8> = kinds.iter().map(|kind| kind.exit_code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), kinds.len(), "códigos de saída duplicados");
    assert!(kinds.iter().all(|kind| !kind.as_str().is_empty()));
}

#[test]
fn io_error_chains_source_and_keeps_path() {
    let error = Error::io(
        "a/b.txt",
        io::Error::new(io::ErrorKind::NotFound, "ausente"),
    );
    assert_eq!(error.kind(), ErrorKind::Io);
    let source = std::error::Error::source(&error);
    assert!(source.is_some(), "o I/O deve encadear a causa");
    assert!(error.to_string().contains("a/b.txt"));
}

#[test]
fn tool_outcome_success_axis() {
    let denied = ToolOutcome::Denied {
        rule_id: RuleId::from("r"),
        evidence: Evidence::new("facto", "argumento", RuleId::from("r")),
    };
    let unavailable = ToolOutcome::Unavailable {
        control: ControlId::new("memoria"),
    };
    assert!(ToolOutcome::Ok.is_success());
    assert!(ToolOutcome::Partial.is_success());
    assert!(!denied.is_success());
    assert!(!ToolOutcome::Timeout.is_success());
    assert!(!unavailable.is_success());
}
