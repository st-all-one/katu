//! `katu-policy` — E02: o motor de política do katu.
//!
//! Avalia **factos tipados** e devolve um veredicto determinístico, sem I/O (DF2). O vocabulário de
//! regras é **fechado e versionado**: `RuleScope` e `Enforcement` são a superfície inteira; alargar
//! é uma decisão de kernel registada, nunca configuração de utilizador.

#![forbid(unsafe_code)]

/// Versão do vocabulário de regras (`POLICY_VOCAB_VERSION`).
///
/// O motor recusa um `RuleSet` com uma versão desconhecida (fail-closed, E02).
pub const POLICY_VOCAB_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::POLICY_VOCAB_VERSION;
    use std::hint::black_box;

    #[test]
    fn vocab_version_is_tracked() {
        let version = black_box(POLICY_VOCAB_VERSION);
        assert_eq!(version, 1);
    }
}
