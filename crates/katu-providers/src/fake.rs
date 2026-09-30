//! Provider **fake** determinístico (E12-T05): o loop corre sem rede nem chave.
//!
//! Reproduz turnos pré-guionados, em ordem. Cada chamada a [`Provider::stream`] consome o turno
//! seguinte; se o guião se esgotar, devolve [`ProviderError::Unsupported`]. Sem I/O, sem `unsafe`.

use std::sync::{Mutex, PoisonError};

use katu_core::provider::{
    Flow, Provider, ProviderError, ProviderEvent, ProviderOutcome, ProviderRequest, ProviderSink,
    StopReason,
};

/// Um turno guionado: eventos (na ordem) + motivo de paragem.
#[derive(Debug, Clone)]
pub struct Turn {
    /// Eventos emitidos no turno.
    pub events: Vec<ProviderEvent>,
    /// Motivo de paragem devolvido no fim.
    pub stop: StopReason,
}

impl Turn {
    /// Turno que só diz `text` e termina.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self {
            events: vec![ProviderEvent::Text(text.to_string())],
            stop: StopReason::EndTurn,
        }
    }
}

/// Provider que reproduz turnos guionados (determinístico).
#[derive(Debug)]
pub struct FakeProvider {
    id: &'static str,
    turns: Vec<Turn>,
    cursor: Mutex<usize>,
}

impl FakeProvider {
    /// Constrói a partir de um guião.
    #[must_use]
    pub fn new(id: &'static str, turns: Vec<Turn>) -> Self {
        Self {
            id,
            turns,
            cursor: Mutex::new(0),
        }
    }

    /// Guião de um só turno textual.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self::new("fake", vec![Turn::text(text)])
    }
}

impl Provider for FakeProvider {
    fn id(&self) -> &str {
        self.id
    }

    fn stream(
        &self,
        _request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        let index = {
            let mut cursor = self.cursor.lock().unwrap_or_else(PoisonError::into_inner);
            let index = *cursor;
            *cursor = cursor.saturating_add(1);
            index
        };
        let turn = self
            .turns
            .get(index)
            .ok_or_else(|| ProviderError::Unsupported("guiao do fake esgotado".to_string()))?;
        for event in &turn.events {
            if matches!(sink.on_event(event.clone()), Flow::Break) {
                return Err(ProviderError::Cancelled);
            }
        }
        Ok(ProviderOutcome {
            usage: None,
            stop: turn.stop.clone(),
        })
    }
}
