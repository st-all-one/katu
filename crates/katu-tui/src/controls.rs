//! Controlos do core na UI (E10-T07/E12-T10): **modelo** e **grau de pensamento**.
//!
//! Só o **utilizador** os muda (o agente nunca se auto-escala); a borda aplica a escolha ao
//! **próximo** turno. O log/estado do kernel não muda por trocar de modelo.

use katu_core::provider::Thinking;

use crate::message::Command;

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

    /// Publica a lista de modelos (o **primeiro** é o default).
    pub(crate) fn set_models(&mut self, models: Vec<String>) {
        self.models = models;
        self.index = 0;
    }

    /// Avança para o próximo modelo; `None` se ainda não há lista (a tecla não faz nada).
    pub(crate) fn cycle_model(&mut self) -> Option<Command> {
        if self.models.is_empty() {
            return None;
        }
        let next = self.index.checked_add(1).unwrap_or(0);
        self.index = if next >= self.models.len() { 0 } else { next };
        let model = self.models.get(self.index)?.clone();
        Some(Command::SetModel(model))
    }

    /// Avança o grau de pensamento no ciclo (o utilizador aciona; E12-T10).
    pub(crate) fn cycle_thinking(&mut self) -> Command {
        self.reasoning = next_thinking(self.reasoning);
        Command::SetThinking(self.reasoning)
    }
}

/// Próximo grau de pensamento no ciclo (E10-T07/E12-T10).
const fn next_thinking(thinking: Thinking) -> Thinking {
    match thinking {
        Thinking::Off => Thinking::Low,
        Thinking::Low => Thinking::Medium,
        Thinking::Medium => Thinking::High,
        _ => Thinking::Off,
    }
}
