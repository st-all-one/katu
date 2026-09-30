//! Contexto efetivo do turno e compactação (E09-T01/T07).

use katu_core::context::{Compaction, CompactionMode, Context};
use katu_core::kernel::SessionError;

use super::Runtime;

impl Runtime<'_> {
    /// Pré-visualiza a compactação do histórico (E09-T07): determinística, sem I/O.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    pub(crate) fn compaction_preview(&self) -> Result<Option<Compaction>, SessionError> {
        self.session
            .compact_context(self.budget, CompactionMode::Enabled)
    }

    /// Monta o **contexto efetivo** do turno (E09-T01/T07): prime + (digest) + sufixo cru.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    pub(crate) fn context(&self) -> Result<Context, SessionError> {
        self.session.context(self.budget, self.compaction)
    }

    /// Modo de compactação corrente (E09-T07).
    pub(crate) const fn compaction(&self) -> CompactionMode {
        self.compaction
    }

    /// Liga/desliga a compactação — **explícito** do utilizador, nunca automático (E09-T07).
    pub(crate) const fn set_compaction(&mut self, mode: CompactionMode) {
        self.compaction = mode;
    }
}
