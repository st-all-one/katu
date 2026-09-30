//! Controlo de modelo e grau de pensamento em runtime (E12-T10).
//!
//! O **utilizador** aciona (TUI/CLI); o agente **não** se auto-escala. O evento fica no log e o
//! estado reflete-o, pelo que sobrevive a *resume*. A validação é pura e o erro **ensina**: nomeia
//! o modelo e diz o que fazer. As capacidades vêm do catálogo pela borda (E12-T02) — o kernel não
//! conhece providers.

use serde::{Deserialize, Serialize};

use crate::provider::{ModelCapabilities, Thinking};

/// Controlo do utilizador em runtime (E12-T10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "control")]
#[non_exhaustive]
pub enum Control {
    /// Define o modelo ativo.
    SetModel {
        /// Identificador do modelo no endpoint.
        model: String,
    },
    /// Define o grau de pensamento.
    SetThinking {
        /// Grau pedido.
        thinking: Thinking,
    },
}

/// Estado do controlo: modelo ativo + grau de pensamento (default `off`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ControlState {
    /// Modelo ativo; `None` = default do adaptador.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Grau de pensamento.
    #[serde(default)]
    pub thinking: Thinking,
}

/// Recusa de um controlo, com uma mensagem que **ensina** (E12-T10).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ControlError {
    /// O modelo-alvo não suporta raciocínio.
    #[error(
        "o modelo `{model}` não suporta raciocínio; define o pensamento como `off` antes de o usar"
    )]
    ReasoningUnsupported {
        /// Modelo-alvo.
        model: String,
    },
}

impl Control {
    /// Valida o controlo contra as capacidades do modelo-alvo (puro; erro que ensina).
    ///
    /// `caps` são as capacidades do modelo **alvo**: o novo, em [`Control::SetModel`]; o corrente,
    /// em [`Control::SetThinking`].
    ///
    /// # Errors
    /// [`ControlError::ReasoningUnsupported`] se o grau de pensamento exigir raciocínio que o
    /// modelo não tem.
    pub fn validate(
        &self,
        current: &ControlState,
        caps: &ModelCapabilities,
    ) -> Result<(), ControlError> {
        let _span = crate::trace_fn!("kernel::control::validate");

        let thinking = match self {
            Self::SetThinking { thinking } => *thinking,
            Self::SetModel { .. } => current.thinking,
        };
        if thinking != Thinking::Off && !caps.reasoning {
            return Err(ControlError::ReasoningUnsupported {
                model: caps.model.clone(),
            });
        }
        Ok(())
    }

    /// Estado resultante de aplicar o controlo (determinístico; sem validação).
    #[must_use]
    pub fn apply(&self, current: &ControlState) -> ControlState {
        let _span = crate::trace_fn!("kernel::control::apply");

        let mut next = current.clone();
        match self {
            Self::SetModel { model } => next.model = Some(model.clone()),
            Self::SetThinking { thinking } => next.thinking = *thinking,
        }
        next
    }

    /// Resumo legível e estável (para a UI; evita `match` não-exaustivo fora da crate).
    #[must_use]
    pub fn summary(&self) -> String {
        let _span = crate::trace_fn!("kernel::control::summary");

        match self {
            Self::SetModel { model } => format!("modelo: {model}"),
            Self::SetThinking { thinking } => format!("pensamento: {}", thinking.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Control, ControlState};
    use crate::provider::{ModelCapabilities, Thinking};

    fn with_reasoning() -> ModelCapabilities {
        ModelCapabilities {
            model: "m".to_string(),
            reasoning: true,
        }
    }

    fn without_reasoning() -> ModelCapabilities {
        ModelCapabilities {
            model: "m".to_string(),
            reasoning: false,
        }
    }

    #[test]
    fn thinking_on_a_non_reasoning_model_is_refused_with_a_teaching_error() {
        let current = ControlState::default();
        let control = Control::SetThinking {
            thinking: Thinking::Low,
        };
        let error = control.validate(&current, &without_reasoning());
        assert!(error.is_err());
        assert!(
            error
                .err()
                .is_some_and(|err| err.to_string().contains("não suporta raciocínio"))
        );
        assert!(
            Control::SetThinking {
                thinking: Thinking::Off
            }
            .validate(&current, &without_reasoning())
            .is_ok()
        );
        assert!(
            Control::SetThinking {
                thinking: Thinking::High
            }
            .validate(&current, &with_reasoning())
            .is_ok()
        );
    }

    #[test]
    fn switching_to_a_non_reasoning_model_with_thinking_on_is_refused() {
        let current = ControlState {
            model: Some("old".to_string()),
            thinking: Thinking::Medium,
        };
        let control = Control::SetModel {
            model: "new".to_string(),
        };
        assert!(control.validate(&current, &without_reasoning()).is_err());
    }

    #[test]
    fn apply_is_deterministic_and_keeps_the_other_field() {
        let current = ControlState {
            model: Some("a".to_string()),
            thinking: Thinking::Low,
        };
        assert_eq!(
            Control::SetModel {
                model: "b".to_string()
            }
            .apply(&current),
            ControlState {
                model: Some("b".to_string()),
                thinking: Thinking::Low,
            }
        );
        assert_eq!(
            Control::SetThinking {
                thinking: Thinking::High
            }
            .apply(&current),
            ControlState {
                model: Some("a".to_string()),
                thinking: Thinking::High,
            }
        );
    }
}
