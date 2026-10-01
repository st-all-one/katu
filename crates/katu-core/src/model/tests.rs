//! Testes das projeções model-facing (outcome, erro, verificação).

use crate::error::{Error, ToolOutcome};
use crate::toon::{emit, project};
use crate::verify::{Check, CheckStatus, VERIFICATION_SCHEMA_VERSION, VerificationReport};
use katu_policy::{ControlId, Evidence, RuleId};

#[test]
fn denied_outcome_carries_rule_and_argument() {
    let rule = RuleId::from("mem-recall-before-write");
    let outcome = ToolOutcome::Denied {
        rule_id: rule.clone(),
        evidence: Evidence::new("sem recall", "/work/src/lib.rs", rule),
    };
    assert_eq!(outcome.status_str(), "denied");
    let rendered = emit(&project(&outcome.to_value()));
    assert!(rendered.contains("\u{1e}k\n"), "{rendered}");
    assert!(rendered.contains("status\u{1f}denied"), "{rendered}");
    assert!(rendered.contains("mem-recall-before-write"), "{rendered}");
}

#[test]
fn unavailable_outcome_names_the_control() {
    let outcome = ToolOutcome::Unavailable {
        control: ControlId::new("approval"),
        rule_id: None,
    };
    assert!(outcome.summary().starts_with("unavailable approval"));
    assert!(outcome.summary().contains("fix:"), "{}", outcome.summary());
    let rendered = emit(&project(&outcome.to_value()));
    assert!(rendered.contains("ctrl\u{1f}approval"), "{rendered}");
    assert!(rendered.contains("fix\u{1f}"), "{rendered}");
}

#[test]
fn outcome_summary_is_not_debug() {
    let outcome = ToolOutcome::Partial;
    assert_eq!(outcome.summary(), "partial");
    assert!(!outcome.summary().contains("Partial"));
}

#[test]
fn denied_summary_includes_the_fix() {
    let rule = RuleId::from("contain-read-outside-workspace");
    let evidence = Evidence::new("fora da raiz", "/etc/passwd", rule)
        .with_remedy(Some("leia só sob a raiz do workspace".to_string()));
    let denied = ToolOutcome::Denied {
        rule_id: RuleId::from("contain-read-outside-workspace"),
        evidence,
    };
    let summary = denied.summary();
    assert!(
        summary.contains("denied contain-read-outside-workspace"),
        "{summary}"
    );
    assert!(
        summary.contains("fix: leia só sob a raiz do workspace"),
        "{summary}"
    );
}

#[test]
fn denied_summary_without_remedy_has_no_fix_suffix() {
    let denied = ToolOutcome::Denied {
        rule_id: RuleId::from("r"),
        evidence: Evidence::new("facto", "argumento", RuleId::from("r")),
    };
    assert_eq!(denied.summary(), "denied r argumento");
}

#[test]
fn error_projects_kind_and_message() {
    let err = Error::invalid_input("campo `range` inválido");
    let rendered = emit(&project(&err.to_value()));
    assert!(rendered.contains("kind\u{1f}invalid_input"), "{rendered}");
    assert!(rendered.contains("range"), "{rendered}");
}

#[test]
fn verification_projects_checks_table() {
    let report = VerificationReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        checks: vec![Check::new(
            "scope",
            CheckStatus::Pass,
            "tudo dentro do escopo",
        )],
        status: CheckStatus::Pass,
        coverage_bps: 10_000,
        strict: false,
    };
    let rendered = emit(&project(&report.to_value()));
    assert!(rendered.contains("\u{1e}checks\n"), "{rendered}");
    assert!(rendered.contains("scope\u{1f}pass"), "{rendered}");
}
