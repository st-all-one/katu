//! Controlos do core na UI (E10-T07/E12-T10): **modelo** e **grau de pensamento**.
//!
//! Só o **utilizador** os muda (o agente nunca se auto-escala); a borda aplica a escolha ao
//! **próximo** turno. O log/estado do kernel não muda por trocar de modelo. Na TUI v2 a escolha
//! faz-se pelos mini-menus de `/model` e `/thinking` (E20-T10).

use katu_core::provider::Thinking;

/// Seleção de modelo e grau de pensamento para os próximos turnos.
#[derive(Debug, Default)]
pub(crate) struct Controls {
    reasoning: Thinking,
    models: Vec<String>,
    index: usize,
}

impl Controls {
    /// Estado inicial: pensamento desligado, sem lista de modelos publicada.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Grau de pensamento escolhido.
    #[must_use]
    pub(crate) const fn reasoning(&self) -> Thinking {
        self.reasoning
    }

    /// Modelo selecionado, se a borda já publicou a lista (E10-T07).
    #[must_use]
    pub(crate) fn model(&self) -> Option<&str> {
        self.models.get(self.index).map(String::as_str)
    }

    /// Lista de modelos publicada pela borda (E12-T02/T10).
    #[must_use]
    pub(crate) fn models(&self) -> &[String] {
        &self.models
    }

    /// Publica a lista de modelos (o **primeiro** é o default).
    pub(crate) fn set_models(&mut self, models: Vec<String>) {
        self.models = models;
        self.index = 0;
    }

    /// Seleciona um modelo da lista publicada (se existir).
    pub(crate) fn set_model(&mut self, model: &str) {
        if let Some(index) = self.models.iter().position(|name| name == model) {
            self.index = index;
        }
    }

    /// Define o grau de pensamento escolhido no menu.
    pub(crate) fn set_thinking(&mut self, thinking: Thinking) {
        self.reasoning = thinking;
    }
}
