//! Consulta e ciclo de vida derivado da sessão: histórico, invariante, checkpoint e *fork*.
//!
//! Métodos puros sobre o log/estado (não mutam o log); separados de `mod.rs` para manter os
//! ficheiros dentro do limite e por coesão.

use std::path::Path;

use super::{Session, SessionError};
use crate::kernel::Event;
use crate::kernel::checkpoint::{self, Checkpoint, CheckpointError};
use crate::kernel::log::{read_records, session_path};
use crate::kernel::project::{Message, derive_messages, state_of};
use crate::ports::FsError;

impl Session<'_> {
    /// Escreve o checkpoint de fase (artefacto durável) a partir do estado corrente.
    ///
    /// # Errors
    /// [`CheckpointError`] se a escrita falhar.
    pub fn write_checkpoint(
        &self,
        goal: &str,
        next_action: &str,
    ) -> Result<Checkpoint, CheckpointError> {
        let checkpoint = Checkpoint::from_state(&self.state, goal, next_action);
        checkpoint::save(self.fs, &self.dir, &checkpoint)?;
        Ok(checkpoint)
    }

    /// Lê o checkpoint de fase, se existir.
    ///
    /// # Errors
    /// [`CheckpointError`] se o ficheiro existir mas não validar.
    pub fn read_checkpoint(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        checkpoint::load(self.fs, &self.dir)
    }

    /// Histórico visível ao modelo, derivado **só** do log (§42).
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn messages(&self) -> Result<Vec<Message>, SessionError> {
        Ok(derive_messages(&self.log_events()?))
    }

    /// Verifica a invariante `Model-visible ⟺ logged` (§42) em runtime: o estado corrente tem de
    /// ser exatamente a projeção do log.
    ///
    /// # Errors
    /// [`SessionError::Invariant`] se o estado divergir do log.
    pub fn verify(&self) -> Result<(), SessionError> {
        let replayed = state_of(&self.log_events()?)?;
        if replayed != self.state {
            return Err(SessionError::Invariant(
                "estado corrente diverge do log".to_string(),
            ));
        }
        Ok(())
    }

    /// Bifurca (*fork*): copia o prefixo do log para outro diretório e retoma lá, sem afetar esta
    /// sessão. Fork e resume derivam ambos do **mesmo** log.
    ///
    /// # Errors
    /// [`SessionError`] se a cópia ou a abertura do destino falharem.
    pub fn fork(&self, dst_dir: &Path) -> Result<Self, SessionError> {
        match self.fs.read(&session_path(&self.dir)) {
            Ok(bytes) => self.fs.write_atomic(&session_path(dst_dir), &bytes)?,
            Err(FsError::NotFound) => {}
            Err(other) => return Err(SessionError::Fs(other)),
        }
        Session::open_with_cap(self.fs, dst_dir, self.cost.caps().global)
    }

    /// Lê os eventos do log.
    pub(super) fn log_events(&self) -> Result<Vec<Event>, SessionError> {
        let records = read_records(self.fs, &session_path(&self.dir))?;
        Ok(records.into_iter().map(|record| record.event).collect())
    }
}
