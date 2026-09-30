//! Aprovação **humana** explícita (E07-T05, §33): o agente nunca assina um override.
//!
//! O evento fica no log (auditável) e concede a capacidade mínima derivada da regra; uma
//! assinatura vazia é recusada pelo `step` (fail-closed).

use super::{Session, SessionError};
use crate::diag::{Level, events};
use crate::kernel::event::Event;
use katu_policy::{Capability, RuleId};

impl Session<'_> {
    /// Regista uma **aprovação humana**: concede `capability` com a justificação `reason` assinada
    /// por `granted_by`. Uma assinatura vazia é recusada; o evento fica no log (não herdado).
    ///
    /// # Errors
    /// [`SessionError`] se a assinatura faltar ou o evento não puder ser logado.
    pub fn approve(
        &mut self,
        rule_id: RuleId,
        capability: Capability,
        reason: &str,
        granted_by: &str,
    ) -> Result<(), SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::POLICY_APPROVAL,
            "kernel::session::approve"
        );
        crate::event!(Level::Info, events::POLICY_APPROVAL);
        self.apply(&Event::ApprovalGranted {
            rule_id,
            capability,
            reason: reason.to_string(),
            granted_by: granted_by.to_string(),
        })
    }
}
