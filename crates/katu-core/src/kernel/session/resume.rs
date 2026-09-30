//! Construtores de alto nível de sessão (ADR 0008): criar, listar, retomar e snapshot.
//!
//! `open`/`open_with_cost` continuam a ser a via de baixo nível (um diretório); estes helpers
//! acrescentam a **vinculação ao projeto** (`.katu/`), o **índice temporal** e a **retomada** por
//! id, sem duplicar a lógica de log/estado.

use std::path::Path;

use super::identity::{self, SessionId, SessionMeta};
use super::snapshot::{SNAPSHOT_SCHEMA_VERSION, StateSnapshot};
use super::{Session, SessionError};
use crate::diag::{Level, events};
use crate::kernel::cost::CostCaps;
use crate::ports::Fs;

impl<'a> Session<'a> {
    /// Cria uma sessão nova vinculada a `root`, indexada temporalmente em `.katu/sessions`.
    ///
    /// # Errors
    /// [`SessionError`] se o layout/log falharem.
    pub fn create(
        fs: &'a dyn Fs,
        root: &Path,
        created_ms: u64,
        goal: &str,
    ) -> Result<Self, SessionError> {
        let _span = crate::span!(Level::Debug, events::SESSION_OPEN, "created_ms" => created_ms);
        let meta = identity::create(fs, root, created_ms, goal)?;
        Self::open_with_cost(
            fs,
            &identity::session_dir(root, &meta.id),
            CostCaps::default(),
        )
    }

    /// Retoma a sessão `id` vinculada a `root` (path exato do projeto).
    ///
    /// # Errors
    /// [`SessionError::UnknownSession`] se o id não existir; [`SessionError`] se o log falhar.
    pub fn resume(fs: &'a dyn Fs, root: &Path, id: &SessionId) -> Result<Self, SessionError> {
        let _span = crate::span!(Level::Debug, events::SESSION_RESUME, "id" => id.as_str());
        identity::find(fs, root, id)?
            .ok_or_else(|| SessionError::UnknownSession(id.as_str().to_string()))?;
        Self::open_with_cost(fs, &identity::session_dir(root, id), CostCaps::default())
    }

    /// Lista as sessões de `root` em ordem temporal `(created_ms, id)`.
    ///
    /// # Errors
    /// [`SessionError`] se o índice for ilegível.
    pub fn list(fs: &dyn Fs, root: &Path) -> Result<Vec<SessionMeta>, SessionError> {
        Ok(identity::list(fs, root)?)
    }

    /// Raiz do projeto vinculada (o diretório da sessão se não houver `meta.json`).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Identificador da sessão, se estiver no layout `.katu/`.
    #[must_use]
    pub fn id(&self) -> Option<&SessionId> {
        self.meta.as_ref().map(|meta| &meta.id)
    }

    /// Metadados da sessão, se existirem.
    #[must_use]
    pub fn meta(&self) -> Option<&SessionMeta> {
        self.meta.as_ref()
    }

    /// Grava explicitamente o snapshot do estado corrente (fronteira de fase).
    ///
    /// # Errors
    /// [`SessionError::Fs`] se a escrita falhar.
    pub fn write_snapshot(&self) -> Result<StateSnapshot, SessionError> {
        let snapshot = StateSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            seq: self.log.seq(),
            offset: self.log.offset(),
            budget: self.cost.global().usage(),
            per_tool: self.cost.per_tool_used().clone(),
            state: self.state.clone(),
        };
        super::snapshot::save(self.fs, &self.dir, &snapshot)?;
        Ok(snapshot)
    }
}
