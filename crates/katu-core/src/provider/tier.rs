//! Tier de modelo (E12-T03): classe de custo/capacidade escolhida pela **política**.
//!
//! A fase do caminho único ([`katu_policy::Phase`]) mapeia para um tier em `policy/tiers.toml`
//! (dado versionado, DF3); o catálogo do provider resolve o tier para o primeiro modelo dessa
//! classe, em ordem determinística. Um modelo explícito do utilizador (E12-T10) vence sempre.

use serde::{Deserialize, Serialize};

/// Classe de modelo para a seleção por política (E12-T03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Tier {
    /// Rápido/barato (perguntas, consulta de conhecimento).
    Fast,
    /// Equilibrado — classe por omissão de um modelo sem tier declarado.
    #[default]
    Balanced,
    /// Profundo/caro (planeamento, raciocínio longo).
    Deep,
}

impl Tier {
    /// Nome estável (`fast`/`balanced`/`deep`) — vocabulário do log e da configuração.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Deep => "deep",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Tier;

    #[test]
    fn default_tier_is_balanced() {
        assert_eq!(Tier::default(), Tier::Balanced);
        assert_eq!(Tier::Fast.as_str(), "fast");
        assert_eq!(Tier::Deep.as_str(), "deep");
    }
}
