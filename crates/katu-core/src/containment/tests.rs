use super::{Containment, ContainmentError, ContainmentStatus, Jail, NoJail, SandboxEnforcement};

#[test]
fn mvp_is_soft_and_not_isolated() {
    let status = ContainmentStatus::mvp();
    assert_eq!(status.mode, Containment::Soft);
    assert_eq!(status.enforcement, SandboxEnforcement::Soft);
    assert!(!status.kernel_isolated());
    assert_eq!(status.mode.as_str(), "soft");
    assert_eq!(status.enforcement.as_str(), "soft");
    assert!(status.mode.enforces_policy());
}

#[test]
fn unconfined_has_no_policy_but_stays_soft_enforcement() {
    assert_eq!(
        Containment::Unconfined.enforcement(),
        SandboxEnforcement::Soft
    );
    assert!(!Containment::Unconfined.enforces_policy());
    assert_eq!(Containment::Unconfined.as_str(), "unconfined");
}

#[test]
fn kernel_isolation_is_only_full_or_partial() {
    assert!(!SandboxEnforcement::Soft.is_kernel_isolated());
    assert!(SandboxEnforcement::Full.is_kernel_isolated());
    assert!(SandboxEnforcement::Partial.is_kernel_isolated());
}

#[test]
fn no_jail_fails_closed_for_kernel_modes() {
    assert_eq!(NoJail.acquire(SandboxEnforcement::Soft), Ok(()));
    assert_eq!(
        NoJail.acquire(SandboxEnforcement::Full),
        Err(ContainmentError::Unavailable {
            mode: SandboxEnforcement::Full
        })
    );
    assert_eq!(
        NoJail.acquire(SandboxEnforcement::Partial),
        Err(ContainmentError::Unavailable {
            mode: SandboxEnforcement::Partial
        })
    );
    let message = ContainmentError::Unavailable {
        mode: SandboxEnforcement::Full,
    }
    .to_string();
    assert!(message.contains("full"), "{message}");
}

#[test]
fn status_serializes_honestly() -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(&ContainmentStatus::mvp())?;
    assert!(json.contains("\"mode\":\"soft\""), "{json}");
    assert!(json.contains("\"enforcement\":\"soft\""), "{json}");
    Ok(())
}
