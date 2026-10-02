//! Testes do núcleo de login (E21).

use super::{Intent, Provider, non_empty, parse, resolve};

#[test]
fn parse_accepts_canonical_names_and_aliases() {
    assert_eq!(parse("opencode"), Some(Provider::OpenCodeGo));
    assert_eq!(parse("opencode-go"), Some(Provider::OpenCodeGo));
    assert_eq!(parse("go"), Some(Provider::OpenCodeGo));
    assert_eq!(parse("opencode-zen"), Some(Provider::OpenCodeZen));
    assert_eq!(parse("zen"), Some(Provider::OpenCodeZen));
    assert_eq!(parse("llama"), Some(Provider::Llama));
    assert_eq!(parse("llama.cpp"), Some(Provider::Llama));
    assert_eq!(parse("local"), Some(Provider::Llama));
    assert_eq!(parse("nope"), None);
}

#[test]
fn only_opencode_needs_a_key() {
    assert!(Provider::OpenCodeGo.needs_key());
    assert!(Provider::OpenCodeZen.needs_key());
    assert!(!Provider::Llama.needs_key());
}

#[test]
fn resolve_reports_unknown_provider_with_the_valid_ones() {
    let error = resolve(Some("nope"), None, None, None, Intent::Login).err();
    let message = error.map(|error| error.to_string()).unwrap_or_default();
    assert!(message.contains("opencode"), "{message}");
    assert!(message.contains("llama"), "{message}");
}

#[test]
fn resolve_logout_without_provider_is_logout() {
    let request = resolve(None, None, None, None, Intent::Logout);
    assert!(matches!(request, Ok(ref request) if request.logout));
}

#[test]
fn non_empty_trims_and_rejects_blanks() {
    assert_eq!(non_empty(Some("  sk-1  ")), Some("sk-1".to_string()));
    assert_eq!(non_empty(Some("   ")), None);
    assert_eq!(non_empty(None), None);
}
