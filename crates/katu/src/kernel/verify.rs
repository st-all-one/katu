//! Gate de verificação e override humano na TUI (E09-T03).
//!
//! O gate corre **dentro** da sessão viva (os factos vêm do log) e é puro (sem LLM). Se bloquear,
//! cada verificação bloqueada pede um override por challenge-and-response (§33); só um humano
//! assina (`granted_by`) e o registo fica append-only em `overrides.jsonl`.

use katu_core::api::Event as Update;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::audit_dir;
use katu_core::verify::{CheckStatus, Override, VerificationReport, append_override, save};

use super::{BusSink, Kernel};
use crate::runtime::VerifyRequest;

impl Kernel<'_> {
    /// Corre o gate de verificação (E09-T03) e, se bloquear, pede override humano assinado.
    pub(super) fn verify(&mut self, sink: &mut BusSink<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Debug, events::VERIFY_REPORT, "verify::verify");
        let report = match self.runtime.verify(VerifyRequest {
            coverage_floor_bps: 0,
            strict: false,
        }) {
            Ok(report) => report,
            Err(error) => return vec![Update::Error(Error::from(error).to_string())],
        };
        let dir = audit_dir(self.runtime.root());
        if let Err(error) = save(self.fs, &dir, &report) {
            return vec![Update::Error(error.to_string())];
        }
        if let Err(error) = self.runtime.record_verification(&report) {
            return vec![Update::Error(error.to_string())];
        }
        let mut updates = vec![Update::Info(format!(
            "verificação: {} · cobertura {} bps",
            report.status.as_str(),
            report.coverage_bps
        ))];
        for check in &report.checks {
            if check.status != CheckStatus::Pass {
                updates.push(Update::Info(format!(
                    "{} {}: {}",
                    check.status.as_str(),
                    check.id,
                    check.detail
                )));
            }
        }
        if report.is_blocked() {
            updates.extend(self.overrides(sink, &report));
        }
        updates
    }

    /// Pede um override por cada bloqueio e regista-o em `overrides.jsonl` (E09-T03, §33).
    pub(super) fn overrides(
        &self,
        sink: &mut BusSink<'_>,
        report: &VerificationReport,
    ) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::verify::overrides");

        let dir = audit_dir(self.runtime.root());
        let now = self.runtime.clock.now().as_millis();
        let mut updates = Vec::new();
        for check in &report.checks {
            if check.status != CheckStatus::Block {
                continue;
            }
            let Some(grant) = sink.ask("verify", &check.id, &check.detail) else {
                updates.push(Update::Info(format!("{}: bloqueio mantido", check.id)));
                continue;
            };
            match Override::new(check.id.clone(), grant.reason, grant.granted_by, now) {
                Ok(signed) => match append_override(self.fs, &dir, &signed) {
                    Ok(()) => {
                        katu_core::event!(
                            Level::Warn,
                            events::VERIFY_OVERRIDE,
                            "check" => check.id.as_str()
                        );
                        updates.push(Update::Info(format!("override registado: {}", check.id)));
                    }
                    Err(error) => updates.push(Update::Error(error.to_string())),
                },
                Err(error) => updates.push(Update::Error(error.to_string())),
            }
        }
        updates
    }
}
