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
