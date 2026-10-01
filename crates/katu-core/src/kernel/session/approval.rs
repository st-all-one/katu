//! Aprovação **humana** explícita (E07-T05, §33): o agente nunca assina um override.
//!
//! O evento fica no log (auditável) e concede a capacidade mínima derivada da regra; uma
//! assinatura vazia é recusada pelo `step` (fail-closed).

use super::{Session, SessionError};
use crate::diag::{Level, events};
use crate::kernel::event::Event;
use crate::kernel::state::RefusalReason;
use crate::kernel::step::refuse;
use crate::report::fingerprint;
use katu_policy::{Capability, RuleId};

/// Computa o MAC de uma aprovação (D3): FNV-1a do conteúdo + chave secreta.
///
/// O MAC é determinístico: a mesma aprovação + a mesma chave → o mesmo signature. Sem chave,
/// a aprovação é recusada (fail-closed).
fn mac(
    rule_id: &RuleId,
    capability: &Capability,
    reason: &str,
    granted_by: &str,
    key: &str,
) -> String {
    let _span = crate::trace_fn!("kernel::session::approval::mac");

    let capability_str = format!("{capability:?}");
    let content = format!(
        "{}|{capability_str}|{reason}|{granted_by}|{key}",
        rule_id.as_str(),
    );
    format!("{:016x}", fingerprint(content.as_bytes()))
}

impl Session<'_> {
    /// Regista uma **aprovação humana**: concede `capability` com a justificação `reason` assinada
    /// por `granted_by`. Uma assinatura vazia é recusada; o evento fica no log (não herdado).
    ///
    /// **D3:** a aprovação é assinada com um MAC (hash do conteúdo + chave secreta). A chave é
    /// passada pelo chamador (vem da config `audit.mac_key`). Sem chave, a aprovação é recusada
    /// (fail-closed).
    ///
    /// # Errors
    /// [`SessionError`] se a assinatura faltar, a chave for vazia ou o evento não puder ser logado.
    #[allow(
        clippy::too_many_arguments,
        reason = "a aprovação carrega o contexto do turno (regra, capacidade, justificação, assinante e chave MAC) sem o esconder num struct de vida curta"
    )]
    pub fn approve(
        &mut self,
        rule_id: RuleId,
        capability: Capability,
        reason: &str,
        granted_by: &str,
        key: &str,
    ) -> Result<(), SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::POLICY_APPROVAL,
            "kernel::session::approve"
        );
        if key.is_empty() {
            return Err(SessionError::Refusal(refuse(
                &self.state,
                RefusalReason::MissingMacKey,
            )));
        }
        let signature = mac(&rule_id, &capability, reason, granted_by, key);
        crate::event!(Level::Info, events::POLICY_APPROVAL);
        self.apply(&Event::ApprovalGranted {
            rule_id,
            capability,
            reason: reason.to_string(),
            granted_by: granted_by.to_string(),
            signature,
        })
    }

    /// Revoga uma capacidade **one-shot** (B-06): a aprovação de escalação de sandbox não é
    /// herdada. Depois de usada, a capacidade é removida e a próxima escalação exige nova
    /// aprovação.
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub fn revoke_approval(&mut self, capability: &Capability) -> Result<(), SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::POLICY_APPROVAL,
            "kernel::session::revoke_approval"
        );
        crate::event!(Level::Info, events::POLICY_APPROVAL);
        self.apply(&Event::ApprovalRevoked {
            capability: capability.clone(),
        })
    }
}
