//! Gate de verificação **determinístico** (E09-T03): função pura sobre (regras, escopo, feedback,
//! diff). **Zero LLM** — a decisão sai de factos tipados, nunca de prosa.
//!
//! Um `block` **não** é sobreponível pelo agente: só um humano assina um [`Override`]
//! (`reason` + `overridden_by`), e o registo fica append-only em `overrides.jsonl`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use crate::feedback::CommandRecord;
use crate::plan::ScopeContract;
use crate::ports::{Fs, FsError};

mod checks;

use checks::{coverage_bps, coverage_check, feedback_checks, scope_checks};
/// Versão do esquema do relatório de verificação.
pub const VERIFICATION_SCHEMA_VERSION: u32 = 1;

/// Escala da cobertura (pontos base).
const BPS: u32 = 10_000;

/// Estado de uma verificação (ordem crescente de gravidade: `Pass < Warn < Block`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CheckStatus {
    /// Verificação passou.
    Pass,
    /// Aviso (não bloqueia, salvo `--strict`).
    Warn,
    /// Bloqueia a transição para `Verified`.
    Block,
}

impl CheckStatus {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Block => "block",
        }
    }

    /// Promove `Warn` a `Block` (usado sob `--strict`, §31).
    #[must_use]
    const fn promoted(self) -> Self {
        match self {
            Self::Warn => Self::Block,
            other => other,
        }
    }
}

/// Uma verificação concreta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// Identificador estável da verificação.
    pub id: String,
    /// Estado.
    pub status: CheckStatus,
    /// Detalhe legível e determinístico.
    pub detail: String,
}

impl Check {
    /// Constrói uma verificação.
    #[must_use]
    pub fn new(id: impl Into<String>, status: CheckStatus, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status,
            detail: detail.into(),
        }
    }
}

/// Entrada do gate: factos já recolhidos (a função é pura).
#[derive(Debug, Clone, Copy)]
pub struct VerificationInput<'a> {
    /// Ficheiros alterados (do diff), **relativos à raiz** do workspace.
    pub changed_files: &'a [String],
    /// Contrato de escopo do plano.
    pub scope: &'a ScopeContract,
    /// Comandos executados (feedback).
    pub commands: &'a [CommandRecord],
    /// Cobertura mínima exigida (pontos base).
    pub coverage_floor_bps: u16,
    /// Modo `--strict`: promove `Warn` a `Block`.
    pub strict: bool,
}

/// Relatório determinístico do gate (o `verification_report.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Verificações, em ordem determinística.
    pub checks: Vec<Check>,
    /// Estado global (o mais grave, já com `strict`).
    pub status: CheckStatus,
    /// Cobertura de escopo em pontos base.
    pub coverage_bps: u16,
    /// Se o modo `--strict` estava ativo.
    pub strict: bool,
}

impl VerificationReport {
    /// `true` se o gate bloqueia a transição para `Verified`.
    #[must_use]
    pub const fn is_blocked(&self) -> bool {
        matches!(self.status, CheckStatus::Block)
    }
}

/// Erro do gate e do override.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// Override sem um campo obrigatório (assinatura incompleta).
    #[error("override sem `{0}`")]
    MissingField(&'static str),
    /// Falha de I/O.
    #[error("I/O da verificação: {0}")]
    Io(#[from] FsError),
    /// Falha de serialização.
    #[error("serialização da verificação: {0}")]
    Serialize(String),
}

/// Override **humano** assinado (o agente não o constrói sem `overridden_by`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    /// Verificação sobreposta.
    pub check_id: String,
    /// Justificação (`override_reason`).
    pub reason: String,
    /// Quem assinou (`overridden_by`).
    pub overridden_by: String,
    /// Instante (ms desde a época).
    pub at_millis: u64,
}

impl Override {
    /// Constrói um override; exige `reason` e `overridden_by` não vazios.
    ///
    /// # Errors
    /// [`VerifyError::MissingField`] se a assinatura estiver incompleta.
    pub fn new(
        check_id: impl Into<String>,
        reason: impl Into<String>,
        overridden_by: impl Into<String>,
        at_millis: u64,
    ) -> Result<Self, VerifyError> {
        let reason = reason.into();
        let overridden_by = overridden_by.into();
        if reason.trim().is_empty() {
            return Err(VerifyError::MissingField("reason"));
        }
        if overridden_by.trim().is_empty() {
            return Err(VerifyError::MissingField("overridden_by"));
        }
        Ok(Self {
            check_id: check_id.into(),
            reason,
            overridden_by,
            at_millis,
        })
    }
}

/// Executa o gate (puro, sem I/O, sem LLM).
#[must_use]
pub fn verify(input: &VerificationInput<'_>) -> VerificationReport {
    let _span = crate::span!(Level::Debug, events::VERIFY_REPORT, "strict" => input.strict);
    let mut checks = scope_checks(input);
    checks.extend(feedback_checks(input));
    let coverage_bps = coverage_bps(input);
    checks.push(coverage_check(coverage_bps, input.coverage_floor_bps));
    let status = checks
        .iter()
        .map(|check| {
            if input.strict {
                check.status.promoted()
            } else {
                check.status
            }
        })
        .max()
        .unwrap_or(CheckStatus::Pass);
    VerificationReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        checks,
        status,
        coverage_bps,
        strict: input.strict,
    }
}

/// Caminho do relatório de verificação.
#[must_use]
pub fn report_path(dir: &Path) -> PathBuf {
    dir.join("verification_report.json")
}

/// Caminho do registo append-only de overrides.
#[must_use]
pub fn overrides_path(dir: &Path) -> PathBuf {
    dir.join("overrides.jsonl")
}

/// Grava o relatório de forma atómica.
///
/// # Errors
/// [`VerifyError`] se a serialização ou o I/O falharem.
pub fn save(fs: &dyn Fs, dir: &Path, report: &VerificationReport) -> Result<(), VerifyError> {
    let mut bytes =
        serde_json::to_vec_pretty(report).map_err(|err| VerifyError::Serialize(err.to_string()))?;
    bytes.push(b'\n');
    fs.write_atomic(&report_path(dir), &bytes)?;
    Ok(())
}

/// Regista um override assinado no `overrides.jsonl` (append-only).
///
/// # Errors
/// [`VerifyError`] se a serialização ou o I/O falharem.
pub fn append_override(fs: &dyn Fs, dir: &Path, override_: &Override) -> Result<(), VerifyError> {
    let mut line =
        serde_json::to_vec(override_).map_err(|err| VerifyError::Serialize(err.to_string()))?;
    line.push(b'\n');
    fs.append(&overrides_path(dir), &line)?;
    Ok(())
}

#[cfg(test)]
mod tests;
