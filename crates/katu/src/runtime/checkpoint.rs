//! Checkpoint de fase visível na UI (E09-T02/E10-T06).
//!
//! O checkpoint tipado é um artefacto **durável** (por sessão) reconstruível do `State`; aqui a
//! borda escreve-o no fim do turno (com a próxima ação declarada) e a UI mostra a ação seguinte —
//! o estado mostrado continua a derivar do `State` (fonte única), nunca de variável paralela.

use katu_core::kernel::{Checkpoint, SessionError};

use super::{Runtime, RuntimeError};

impl Runtime<'_> {
    /// Escreve o checkpoint de fase com a próxima ação declarada (E09-T02).
    ///
    /// # Errors
    /// [`RuntimeError::Session`] se a escrita/validação falhar.
    pub(crate) fn write_checkpoint(&self, next_action: &str) -> Result<Checkpoint, RuntimeError> {
        self.session
            .write_checkpoint(&self.goal, next_action)
            .map_err(|error| RuntimeError::Session(SessionError::from(error)))
    }

    /// Lê o checkpoint de fase corrente, se existir.
    ///
    /// # Errors
    /// [`RuntimeError::Session`] se o ficheiro existir mas não validar.
    pub(crate) fn checkpoint(&self) -> Result<Option<Checkpoint>, RuntimeError> {
        self.session
            .read_checkpoint()
            .map_err(|error| RuntimeError::Session(SessionError::from(error)))
    }
}
