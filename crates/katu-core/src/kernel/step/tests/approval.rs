//! E07-T05 — aprovação humana: capacidade mínima concedida e assinatura obrigatória.

use super::step;
use crate::kernel::event::Event;
use crate::kernel::state::{RefusalReason, State};
use katu_policy::{Capability, ResolvedPath, RuleId};

#[test]
fn approval_grants_a_capability_and_requires_a_signature() -> Result<(), Box<dyn std::error::Error>>
{
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let event = |reason: &str, granted_by: &str| Event::ApprovalGranted {
        rule_id: RuleId::from("contain-read-outside-workspace"),
        capability: capability.clone(),
        reason: reason.to_string(),
        granted_by: granted_by.to_string(),
        signature: "test-sig".to_string(),
    };

    let granted = step(&State::initial(), &event("necessário para o teste", "ana"))?;
    assert!(granted.capabilities.contains(&capability));

    for (reason, granted_by) in [("  ", "ana"), ("motivo", "  ")] {
        let refused = step(&State::initial(), &event(reason, granted_by));
        assert!(
            matches!(refused, Err(refusal) if refusal.reason == RefusalReason::UnsignedApproval),
            "assinatura vazia tem de ser recusada"
        );
    }
    Ok(())
}

#[test]
fn approval_revoked_removes_the_capability() -> Result<(), Box<dyn std::error::Error>> {
    // B-06: a revogação one-shot remove a capacidade do estado.
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let granted = step(
        &State::initial(),
        &Event::ApprovalGranted {
            rule_id: RuleId::from("contain-read-outside-workspace"),
            capability: capability.clone(),
            reason: "necessário".to_string(),
            granted_by: "ana".to_string(),
            signature: "test-sig".to_string(),
        },
    )?;
    assert!(granted.capabilities.contains(&capability));

    let revoked = step(
        &granted,
        &Event::ApprovalRevoked {
            capability: capability.clone(),
        },
    )?;
    assert!(
        !revoked.capabilities.contains(&capability),
        "a revogação one-shot tem de remover a capacidade"
    );
    Ok(())
}
