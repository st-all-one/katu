//! Testes do vocabulário de regras (round-trip TOML e versão).

use super::{
    Enforcement, PolicyError, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet,
    Severity,
};
use crate::paths::ResolvedPath;

pub(crate) fn sample_rules() -> Result<RuleSet, PolicyError> {
    let root = ResolvedPath::from_canonical("/work/secrets")?;
    Ok(RuleSet {
        vocab: 3,
        rules: vec![Rule {
            id: RuleId::from("no-write-secrets"),
            statement: "não escrever em segredos".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            remedy: None,
            expires_at: None,
            waiver: None,
            examples: RuleExamples {
                negative: vec!["write /work/secrets/token".to_string()],
                positive: vec!["write /work/src/main.rs".to_string()],
            },
        }],
    })
}

#[test]
fn toml_round_trip_preserves_rules() -> Result<(), PolicyError> {
    let original = sample_rules()?;
    let text = original.to_toml()?;
    let parsed = RuleSet::from_toml(&text)?;
    assert_eq!(original, parsed);
    Ok(())
}

#[test]
fn unknown_vocab_is_rejected() -> Result<(), PolicyError> {
    let mut rules = sample_rules()?;
    rules.vocab = 999;
    assert!(rules.check_vocab().is_err());
    Ok(())
}
