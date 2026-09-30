//! Gate de verificação sobre o log da sessão (E09-T03).

use katu_core::kernel::SessionError;
use katu_core::verify::{VerificationInput, VerificationReport, verify};

use super::{Runtime, RuntimeError, VerifyRequest};

impl Runtime<'_> {
    /// Corre o **gate de verificação** sobre os factos do log (E09-T03): puro, sem LLM.
    ///
    /// O `diff` são os ficheiros alterados por tools de escrita e o `feedback` são os comandos
    /// registados — ambos derivados do log (`Model-visible ⟺ logged`). Os caminhos vão
    /// **relativos à raiz**, para casarem com os globs do contrato de escopo.
    ///
    /// # Errors
    /// [`RuntimeError::Verification`] se não houver escopo; [`RuntimeError::Session`] se o log falhar.
    pub(crate) fn verify(
        &self,
        request: VerifyRequest,
    ) -> Result<VerificationReport, RuntimeError> {
        let Some(plan) = self.plan() else {
            return Err(RuntimeError::Verification(
                "sem `scope_contract.json` (o gate exige escopo)".to_string(),
            ));
        };
        let files = self.session.changed_files()?;
        let commands = self.session.recorded_commands()?;
        Ok(verify(&VerificationInput {
            changed_files: &files,
            scope: &plan.scope_contract,
            commands: &commands,
            coverage_floor_bps: request.coverage_floor_bps,
            strict: request.strict,
        }))
    }

    /// Regista o relatório no log (E09-T03): auditável e determinístico.
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub(crate) fn record_verification(
        &mut self,
        report: &VerificationReport,
    ) -> Result<(), SessionError> {
        self.session.record_verification(report)
    }
}
