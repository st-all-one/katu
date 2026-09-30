//! Política de tiers (E12-T03): fase do caminho único → classe de modelo, de `policy/tiers.toml`.
//!
//! Dado versionado (DF3), embutido em compilação como as regras de memória/contenção. A seleção é
//! determinística e **nunca** sobrepõe um modelo explícito do utilizador (E12-T10): só corre quando
//! o controlo não fixou modelo.

use katu_core::diag::{Level, events};
use katu_core::provider::{Provider, Tier};
use katu_policy::Phase;
use serde::Deserialize;

/// Versão do vocabulário de tiers (`policy/tiers.toml`); recusada se desconhecida (fail-closed).
pub(crate) const TIER_VOCAB_VERSION: u32 = 1;

/// Política embutida (versionada no repositório).
const TIER_POLICY: &str = include_str!("../../../policy/tiers.toml");

/// Rota: fase → tier.
#[derive(Debug, Deserialize)]
struct TierRoute {
    phase: Phase,
    tier: Tier,
}

/// Documento TOML de `policy/tiers.toml`.
#[derive(Debug, Deserialize)]
struct TierDoc {
    vocab: u32,
    #[serde(default)]
    default: Tier,
    #[serde(default)]
    routes: Vec<TierRoute>,
}

/// Política de tiers carregada: rotas (primeira que casa vence) + tier por omissão.
#[derive(Debug)]
pub(crate) struct TierPolicy {
    routes: Vec<(Phase, Tier)>,
    default: Tier,
}

impl TierPolicy {
    /// Carrega a política embutida.
    ///
    /// # Errors
    /// Mensagem legível se o TOML for inválido ou a versão do vocabulário desconhecida.
    pub(crate) fn load() -> Result<Self, String> {
        let _span = katu_core::trace_fn!("tier::load");

        Self::from_toml(TIER_POLICY)
    }

    /// Carrega de TOML (testável sem o ficheiro real).
    ///
    /// # Errors
    /// Mensagem legível se o TOML for inválido ou a versão do vocabulário desconhecida.
    pub(crate) fn from_toml(text: &str) -> Result<Self, String> {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::POLICY_LOAD,
            "tier::TierPolicy::from_toml"
        );
        let doc: TierDoc =
            toml::from_str(text).map_err(|error| format!("policy/tiers.toml: {error}"))?;
        if doc.vocab != TIER_VOCAB_VERSION {
            return Err(format!(
                "policy/tiers.toml: vocab {} (esperado {TIER_VOCAB_VERSION})",
                doc.vocab
            ));
        }
        Ok(Self {
            routes: doc
                .routes
                .into_iter()
                .map(|route| (route.phase, route.tier))
                .collect(),
            default: doc.default,
        })
    }

    /// Tier da fase (primeira rota que casa; senão, o default).
    #[must_use]
    pub(crate) fn tier_for(&self, phase: Phase) -> Tier {
        let _span = katu_core::trace_fn!("tier::tier_for");

        self.routes
            .iter()
            .find(|(route, _)| *route == phase)
            .map_or(self.default, |(_, tier)| *tier)
    }

    /// Modelo a usar na fase: o primeiro do tier no catálogo do provider, ou o `default` (E12-T03).
    ///
    /// Instrumentado (`provider.tier`); **não** sobrepõe controlo do utilizador — quem chama só
    /// chega aqui quando o controlo não fixou modelo.
    pub(crate) fn model_for(&self, provider: &dyn Provider, phase: Phase, default: &str) -> String {
        let _span = katu_core::trace_fn!("tier::model_for");

        let tier = self.tier_for(phase);
        let selected = provider.model_for_tier(tier);
        katu_core::event!(
            Level::Debug,
            events::PROVIDER_TIER,
            "phase" => phase.as_str(),
            "tier" => tier.as_str(),
            "model" => selected.as_deref().unwrap_or(default),
        );
        selected.unwrap_or_else(|| default.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::TierPolicy;
    use katu_core::provider::Tier;
    use katu_policy::Phase;

    const SAMPLE: &str = r#"
vocab = 1
default = "balanced"
[[routes]]
phase = "planned"
tier = "deep"
"#;

    #[test]
    fn routes_win_over_the_default() -> Result<(), String> {
        let policy = TierPolicy::from_toml(SAMPLE)?;
        assert_eq!(policy.tier_for(Phase::Planned), Tier::Deep);
        assert_eq!(policy.tier_for(Phase::Task), Tier::Balanced);
        Ok(())
    }

    #[test]
    fn unknown_vocab_is_an_error() {
        let text = "vocab = 99\ndefault = \"balanced\"\n";
        assert!(TierPolicy::from_toml(text).is_err());
    }

    #[test]
    fn embedded_policy_loads() -> Result<(), String> {
        let policy = TierPolicy::load()?;
        assert_eq!(policy.tier_for(Phase::Planned), Tier::Deep);
        Ok(())
    }
}
