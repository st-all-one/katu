use super::{Feature, FeatureStatus, Plan, PlanError, ScopeContract};

fn contract() -> ScopeContract {
    ScopeContract::new(
        vec!["src/**".to_string()],
        vec!["**/secrets/**".to_string()],
        vec!["testes passam".to_string()],
        "reverter o commit",
    )
}

fn plan(features: Vec<Feature>) -> Plan {
    Plan::new(contract(), features)
}

fn feature(id: &str, status: FeatureStatus) -> Feature {
    Feature::new(id, "fazer", status)
}

#[test]
fn valid_plan_passes() -> Result<(), PlanError> {
    plan(vec![feature("F1", FeatureStatus::InProgress)]).validate()
}

#[test]
fn empty_feature_list_is_rejected() {
    assert_eq!(
        plan(Vec::new()).validate(),
        Err(PlanError::EmptyFeatureList)
    );
}

#[test]
fn missing_forbidden_files_is_rejected() {
    let mut plan = plan(vec![feature("F1", FeatureStatus::Pending)]);
    plan.scope_contract.forbidden_files.clear();
    assert_eq!(plan.validate(), Err(PlanError::MissingForbiddenFiles));
}

#[test]
fn missing_rollback_is_rejected() {
    let mut plan = plan(vec![feature("F1", FeatureStatus::Pending)]);
    plan.scope_contract.rollback_plan = "   ".to_string();
    assert_eq!(plan.validate(), Err(PlanError::MissingRollbackPlan));
}

#[test]
fn multiple_in_progress_is_rejected() {
    let plan = plan(vec![
        feature("F1", FeatureStatus::InProgress),
        feature("F2", FeatureStatus::InProgress),
    ]);
    assert_eq!(plan.validate(), Err(PlanError::MultipleInProgress));
}

#[test]
fn non_relative_glob_is_rejected() {
    let mut scoped = plan(vec![feature("F1", FeatureStatus::Pending)]);
    scoped
        .scope_contract
        .forbidden_files
        .push("/etc/**".to_string());
    assert_eq!(
        scoped.validate(),
        Err(PlanError::NonRelativeGlob {
            pattern: "/etc/**".to_string()
        })
    );
    let mut escaping = plan(vec![feature("F1", FeatureStatus::Pending)]);
    escaping.scope_contract.allowed_files = vec!["../secrets/**".to_string()];
    assert!(matches!(
        escaping.validate(),
        Err(PlanError::NonRelativeGlob { .. })
    ));
}

#[test]
fn allows_respects_forbidden_first() {
    let contract = contract();
    assert!(contract.allows("src/main.rs"));
    assert!(!contract.allows("src/secrets/token"));
    assert!(!contract.allows("docs/readme.md"));
}
