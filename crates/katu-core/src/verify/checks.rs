//! Verificações concretas do gate (E09-T03): escopo, feedback e cobertura.

use super::{BPS, Check, CheckStatus, VerificationInput};
use crate::diag::{Level, events};
use crate::plan::matches_glob;

/// Verificações de escopo (proibido vence; fora do permitido é aviso).
pub(super) fn scope_checks(input: &VerificationInput<'_>) -> Vec<Check> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::VERIFY_REPORT,
        "verify::checks::scope_checks"
    );
    let mut forbidden = 0usize;
    let mut outside = 0usize;
    for path in input.changed_files {
        if input
            .scope
            .forbidden_files
            .iter()
            .any(|pattern| matches_glob(pattern, path.as_str()))
        {
            forbidden = forbidden.saturating_add(1);
        } else if !input.scope.allows(path.as_str()) {
            outside = outside.saturating_add(1);
        }
    }
    vec![
        if forbidden == 0 {
            Check::new(
                "scope.forbidden",
                CheckStatus::Pass,
                "nenhum ficheiro proibido alterado",
            )
        } else {
            Check::new(
                "scope.forbidden",
                CheckStatus::Block,
                format!("{forbidden} ficheiro(s) proibido(s) alterado(s)"),
            )
        },
        if outside == 0 {
            Check::new(
                "scope.allowed",
                CheckStatus::Pass,
                "todos os ficheiros no escopo permitido",
            )
        } else {
            Check::new(
                "scope.allowed",
                CheckStatus::Warn,
                format!("{outside} ficheiro(s) fora do escopo permitido"),
            )
        },
    ]
}

/// Verificações de feedback de comando (`exit_code: null` bloqueia, §31).
pub(super) fn feedback_checks(input: &VerificationInput<'_>) -> Vec<Check> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::VERIFY_REPORT,
        "verify::checks::feedback_checks"
    );
    let mut timeouts = 0usize;
    let mut ambiguous = 0usize;
    let mut failures = 0usize;
    for record in input.commands {
        let status = record.status();
        if status.timed_out {
            timeouts = timeouts.saturating_add(1);
        } else if status.is_ambiguous() {
            ambiguous = ambiguous.saturating_add(1);
        } else if status.exit_code != Some(0) {
            failures = failures.saturating_add(1);
        }
    }
    vec![
        count_check(
            "feedback.timeout",
            timeouts,
            "comando(s) excederam o tempo",
            "nenhum comando excedeu o tempo",
            CheckStatus::Block,
        ),
        count_check(
            "feedback.ambiguous",
            ambiguous,
            "comando(s) sem código de saída",
            "nenhum comando ambíguo",
            CheckStatus::Block,
        ),
        count_check(
            "feedback.exit",
            failures,
            "comando(s) falharam",
            "todos os comandos terminaram com sucesso",
            CheckStatus::Warn,
        ),
    ]
}

/// Verificação de contagem: `Pass` quando zero, senão `failure`.
fn count_check(
    id: &str,
    count: usize,
    detail_fail: &str,
    detail_pass: &str,
    failure: CheckStatus,
) -> Check {
    let _span = crate::trace_fn!("verify::checks::count_check");

    if count == 0 {
        Check::new(id, CheckStatus::Pass, detail_pass)
    } else {
        Check::new(id, failure, format!("{count} {detail_fail}"))
    }
}

/// Cobertura de escopo: fração dos ficheiros alterados que estão no escopo permitido.
pub(super) fn coverage_bps(input: &VerificationInput<'_>) -> u16 {
    let _span = crate::fn_span!(
        Level::Trace,
        events::VERIFY_REPORT,
        "verify::checks::coverage_bps"
    );
    let total = input.changed_files.len();
    if total == 0 {
        return u16::try_from(BPS).unwrap_or(u16::MAX);
    }
    let in_scope = input
        .changed_files
        .iter()
        .filter(|path| input.scope.allows(path.as_str()))
        .count();
    let in_scope = u32::try_from(in_scope).unwrap_or(u32::MAX);
    let total = u32::try_from(total).unwrap_or(u32::MAX);
    let bps = in_scope
        .saturating_mul(BPS)
        .checked_div(total)
        .unwrap_or(BPS);
    u16::try_from(bps).unwrap_or(u16::MAX)
}

/// Verificação da cobertura contra o piso.
pub(super) fn coverage_check(coverage_bps: u16, floor_bps: u16) -> Check {
    let _span = crate::fn_span!(
        Level::Trace,
        events::VERIFY_REPORT,
        "verify::checks::coverage_check"
    );
    if coverage_bps >= floor_bps {
        Check::new(
            "coverage",
            CheckStatus::Pass,
            format!("cobertura {coverage_bps} bps ≥ piso {floor_bps} bps"),
        )
    } else {
        Check::new(
            "coverage",
            CheckStatus::Warn,
            format!("cobertura {coverage_bps} bps < piso {floor_bps} bps"),
        )
    }
}
