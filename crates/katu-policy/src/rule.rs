//! Regra como **dado versionado** (E02-T02) e o seu `RuleSet`. A aplicação vive em
//! [`crate::engine`].

use serde::{Deserialize, Serialize};

use crate::POLICY_VOCAB_VERSION;
use crate::error::PolicyError;
use crate::facts::{Phase, Timestamp, ToolName};
use crate::paths::ResolvedPath;

/// Identificador estável de regra.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    /// Texto do identificador.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RuleId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for RuleId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Âmbito em que uma regra é considerada (vocabulário **fechado**).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleScope {
    /// Sob uma raiz de caminho.
    Path {
        /// Raiz.
        root: ResolvedPath,
    },
    /// Sobre uma tool.
    Command {
        /// Tool alvo.
        tool: ToolName,
    },
    /// Numa fase do caminho único.
    Phase {
        /// Fase alvo.
        phase: Phase,
    },
    /// Sobre um orçamento.
    Budget {
        /// Teto.
        cap: BudgetCap,
    },
}

/// Teto de orçamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BudgetCap {
    /// Escritas por tarefa.
    Writes(u32),
    /// Bytes por tarefa.
    Bytes(u64),
    /// Execuções por tarefa.
    Execs(u32),
}

/// Forma de aplicação de uma regra (vocabulário **fechado**, 9 variantes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Enforcement {
    /// Nega uma tool.
    DenyCommand {
        /// Tool negada.
        tool: ToolName,
    },
    /// Nega escrita sob uma raiz.
    DenyWrite {
        /// Raiz protegida.
        root: ResolvedPath,
    },
    /// Nega a leitura sob uma raiz.
    DenyRead {
        /// Raiz protegida.
        root: ResolvedPath,
    },
    /// Nega a leitura de caminhos **sensíveis** (glob por componente), salvo `ReadPath` explícito.
    ///
    /// O workspace **não** destranca: `.ssh`/`.env` exigem autorização explícita (E07-T05).
    DenySensitiveRead {
        /// Globs aplicados a cada componente do caminho.
        globs: Vec<String>,
    },
    /// Nega o envio para lixo sob uma raiz.
    DenyDelete {
        /// Raiz protegida.
        root: ResolvedPath,
    },
    /// Exige uma fase antes.
    RequireBefore {
        /// Fase obrigatória.
        phase: Phase,
    },
    /// Exige uma tool antes.
    RequireAfter {
        /// Tool obrigatória.
        tool: ToolName,
    },
    /// Aplica um teto de orçamento.
    Budget {
        /// Teto.
        cap: BudgetCap,
    },
    /// Apenas informa.
    Advisory,
}

/// Severidade da regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Severity {
    /// Crítica (nega).
    Critical,
    /// Aviso (exige aprovação).
    Warn,
}

/// Categoria da regra (auditoria, E02-T04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleCategory {
    /// Aplicada pelo motor.
    Enforced,
    /// Só informa.
    Advisory,
    /// Perceção (não decide).
    Perception,
}

/// Exceção explícita a uma regra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    /// Justificação.
    pub reason: String,
    /// Expiração (opcional).
    pub expires_at: Option<Timestamp>,
}

/// Exemplos da regra: **o negativo é obrigatório** para `Enforced` (§51.7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleExamples {
    /// Um comando/caminho que a regra **nega**.
    #[serde(default)]
    pub negative: Vec<String>,
    /// Um que ela permite.
    #[serde(default)]
    pub positive: Vec<String>,
}

/// Regra como dado versionado (DF3, DF7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Identificador.
    pub id: RuleId,
    /// Enunciado legível.
    pub statement: String,
    /// Âmbito.
    pub scope: RuleScope,
    /// Forma de aplicação.
    pub enforcement: Enforcement,
    /// Severidade.
    pub severity: Severity,
    /// Categoria.
    pub category: RuleCategory,
    /// Expiração (revisão por default a 90 dias).
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
    /// Exceção explícita.
    #[serde(default)]
    pub waiver: Option<Waiver>,
    /// Exemplos.
    #[serde(default)]
    pub examples: RuleExamples,
}

impl Rule {
    /// `true` se a regra está ativa (não expirada nem suspensa por `waiver`).
    #[must_use]
    pub fn is_active(&self, now_millis: u64) -> bool {
        if let Some(expires_at) = self.expires_at
            && now_millis >= expires_at.as_millis()
        {
            return false;
        }
        if let Some(waiver) = &self.waiver
            && waiver
                .expires_at
                .is_none_or(|end| now_millis < end.as_millis())
        {
            return false;
        }
        true
    }
}

/// Conjunto de regras versionado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    /// Versão do vocabulário (`POLICY_VOCAB_VERSION`).
    pub vocab: u32,
    /// Regras.
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// Carrega de TOML e **valida a versão** do vocabulário (fail-closed).
    pub fn from_toml(text: &str) -> Result<Self, PolicyError> {
        let set: Self = toml::from_str(text).map_err(|err| PolicyError::Toml(err.to_string()))?;
        set.check_vocab()?;
        Ok(set)
    }

    /// Serializa para TOML.
    pub fn to_toml(&self) -> Result<String, PolicyError> {
        toml::to_string_pretty(self).map_err(|err| PolicyError::Toml(err.to_string()))
    }

    /// Recusa um vocabulário desconhecido.
    pub fn check_vocab(&self) -> Result<(), PolicyError> {
        if self.vocab == POLICY_VOCAB_VERSION {
            Ok(())
        } else {
            Err(PolicyError::UnknownVocab {
                found: self.vocab,
                expected: POLICY_VOCAB_VERSION,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Enforcement, PolicyError, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet,
        Severity,
    };
    use crate::paths::ResolvedPath;

    pub(crate) fn sample_rules() -> Result<RuleSet, PolicyError> {
        let root = ResolvedPath::from_canonical("/work/secrets")?;
        Ok(RuleSet {
            vocab: 2,
            rules: vec![Rule {
                id: RuleId::from("no-write-secrets"),
                statement: "não escrever em segredos".to_string(),
                scope: RuleScope::Path { root: root.clone() },
                enforcement: Enforcement::DenyWrite { root },
                severity: Severity::Critical,
                category: RuleCategory::Enforced,
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
}
