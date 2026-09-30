//! Gate de verificação e override humano na TUI (E09-T03).
//!
//! O gate corre **dentro** da sessão viva (os factos vêm do log) e é puro (sem LLM). Se bloquear,
//! cada verificação bloqueada pede um override por challenge-and-response (§33); só um humano
//! assina (`granted_by`) e o registo fica append-only em `overrides.jsonl`.

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::audit_dir;
use katu_core::ports::Env;
use katu_core::verify::{CheckStatus, Override, VerificationReport, append_override, save};
use katu_tui::{ChallengePrompt, Painter, Update};

use super::AgentHandler;
use crate::runtime::VerifyRequest;

impl AgentHandler<'_> {
    /// Corre o gate de verificação (E09-T03) e, se bloquear, pede override humano assinado.
    pub(super) fn verify(&mut self, painter: &mut Painter<'_>) -> Vec<Update> {
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
            updates.extend(self.overrides(painter, &report));
        }
        updates
    }

    /// Pede um override por cada bloqueio e regista-o em `overrides.jsonl` (E09-T03, §33).
    pub(super) fn overrides(
        &self,
        painter: &mut Painter<'_>,
        report: &VerificationReport,
    ) -> Vec<Update> {
        let granted_by = self.granted_by();
        let dir = audit_dir(self.runtime.root());
        let now = self.runtime.clock.now().as_millis();
        let mut updates = Vec::new();
        for check in &report.checks {
            if check.status != CheckStatus::Block {
                continue;
            }
            let request = ChallengePrompt {
                tool: "verify".to_string(),
                rule: check.id.clone(),
                scope: check.detail.clone(),
            };
            let Some(signature) = painter.challenge(request, &granted_by) else {
                updates.push(Update::Info(format!("{}: bloqueio mantido", check.id)));
                continue;
            };
            match Override::new(
                check.id.clone(),
                signature.reason,
                signature.granted_by,
                now,
            ) {
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

    /// Quem assina (`USER`/`USERNAME`, ou `local`); o agente **nunca** assina.
    pub(super) fn granted_by(&self) -> String {
        self.env
            .var("USER")
            .or_else(|| self.env.var("USERNAME"))
            .unwrap_or_else(|| "local".to_string())
    }
}
