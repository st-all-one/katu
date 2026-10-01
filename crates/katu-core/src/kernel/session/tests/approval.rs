//! E07-T05 — a aprovação humana fica no log (replay) e uma assinatura vazia é recusada.

use crate::kernel::session::{Session, SessionError};
use crate::ports::MemFs;
use katu_policy::{Capability, ResolvedPath, RuleId};
use std::path::Path;

#[test]
fn approval_survives_replay_and_requires_a_signature() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions-approval");
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let rule = RuleId::from("contain-read-outside-workspace");
    let mut session = Session::open(&fs, dir)?;
    session.approve(rule.clone(), capability.clone(), "necessário", "ana")?;
    session.verify()?;

    let mut reopened = Session::open(&fs, dir)?;
    assert!(reopened.state().capabilities.contains(&capability));

    let unsigned = reopened.approve(rule, capability, "   ", "ana");
    assert!(
        matches!(unsigned, Err(SessionError::Refusal(_))),
        "assinatura vazia tem de ser recusada"
    );
    Ok(())
}

#[test]
fn one_shot_approval_is_revoked_after_use() -> Result<(), Box<dyn std::error::Error>> {
    // B-06: a aprovação one-shot é revogada depois de usada — a capacidade desaparece.
    let fs = MemFs::new();
    let dir = Path::new("/sessions-one-shot");
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let rule = RuleId::from("contain-read-outside-workspace");
    let mut session = Session::open(&fs, dir)?;
    session.approve(rule, capability.clone(), "necessário", "ana")?;
    assert!(session.state().capabilities.contains(&capability));

    session.revoke_approval(&capability)?;
    assert!(
        !session.state().capabilities.contains(&capability),
        "a capacidade one-shot tem de ser revogada"
    );
    Ok(())
}

#[test]
fn one_shot_approval_does_not_survive_replay() -> Result<(), Box<dyn std::error::Error>> {
    // B-06: a revogação é persistente — depois de aberta de novo, a capacidade já não está lá.
    let fs = MemFs::new();
    let dir = Path::new("/sessions-one-shot-replay");
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let rule = RuleId::from("contain-read-outside-workspace");
    let mut session = Session::open(&fs, dir)?;
    session.approve(rule, capability.clone(), "necessário", "ana")?;
    session.revoke_approval(&capability)?;
    session.verify()?;

    let reopened = Session::open(&fs, dir)?;
    assert!(
        !reopened.state().capabilities.contains(&capability),
        "a revogação one-shot tem de sobreviver ao replay"
    );
    Ok(())
}

#[test]
fn a_second_escalation_requires_a_new_approval() -> Result<(), Box<dyn std::error::Error>> {
    // B-06: a aprovação não é herdada — a segunda escalação exige nova aprovação.
    let fs = MemFs::new();
    let dir = Path::new("/sessions-no-reuse");
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let rule = RuleId::from("contain-read-outside-workspace");
    let mut session = Session::open(&fs, dir)?;

    // Primeira aprovação + revogação (one-shot).
    session.approve(rule.clone(), capability.clone(), "primeira", "ana")?;
    assert!(session.state().capabilities.contains(&capability));
    session.revoke_approval(&capability)?;
    assert!(!session.state().capabilities.contains(&capability));

    // Segunda escalação: a capacidade já não está lá — precisa de nova aprovação.
    assert!(
        !session.state().capabilities.contains(&capability),
        "a segunda escalação não pode reutilizar a aprovação anterior"
    );
    session.approve(rule, capability.clone(), "segunda", "ana")?;
    assert!(session.state().capabilities.contains(&capability));
    Ok(())
}

/// A/B determinístico da aprovação one-shot (B-06): escreve o artefacto em `KATU_APPROVAL_OUT`.
#[test]
#[ignore = "bench A/B: escreve o artefacto do protocolo (a via normal é o gate)"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_approval_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions-approval-artifact");
    let capability = Capability::ReadPath {
        root: ResolvedPath::from_canonical("/etc/hosts")?,
    };
    let rule = RuleId::from("contain-read-outside-workspace");
    let mut session = Session::open(&fs, dir)?;

    // Cenário 1: aprovação one-shot é revogada depois de usada.
    session.approve(rule.clone(), capability.clone(), "primeira", "ana")?;
    let granted = session.state().capabilities.contains(&capability);
    session.revoke_approval(&capability)?;
    let revoked = !session.state().capabilities.contains(&capability);

    // Cenário 2: a segunda escalação exige nova aprovação (não é herdada).
    let needs_new = !session.state().capabilities.contains(&capability);
    session.approve(rule, capability.clone(), "segunda", "ana")?;
    let reapproved = session.state().capabilities.contains(&capability);

    let value = serde_json::json!({
        "schema": "katu.bench.approval.v1",
        "question": "a aprovação one-shot de escalação de sandbox não é herdada",
        "rule": "aprovação com justificação, revogada depois de usada; a próxima escalação exige nova aprovação",
        "scenarios": [
            { "name": "one_shot_revoked", "granted": granted, "revoked": revoked },
            { "name": "non_reuse", "needs_new_approval": needs_new, "reapproved": reapproved },
        ],
        "totals": { "one_shot_revoked": granted && revoked, "non_reuse": needs_new && reapproved },
        "criterion": "a aprovação é revogada depois de usada e a segunda escalação exige nova aprovação",
        "criterion_met": granted && revoked && needs_new && reapproved,
        "caveat": "proxy determinístico (sem I/O real de sandbox): mede a revogação e a não-reutilização, não a execução do comando",
        "decision": "default on (a aprovação one-shot é o mecanismo de escalação de sandbox)",
    });
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_APPROVAL_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed.get("criterion_met"), Some(&serde_json::json!(true)));
    Ok(())
}
