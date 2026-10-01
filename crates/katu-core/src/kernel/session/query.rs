//! Consulta e ciclo de vida derivado da sessão: histórico, invariante, checkpoint e *fork*.
//!
//! Métodos puros sobre o log/estado (não mutam o log); separados de `mod.rs` para manter os
//! ficheiros dentro do limite e por coesão.

use std::path::Path;

use super::{Session, SessionError};
use crate::context::{Compaction, CompactionMode, Context, ContextBudget, assemble, compact};
use crate::feedback::CommandRecord;
use crate::kernel::checkpoint::{self, Checkpoint, CheckpointError};
use crate::kernel::log::{read_records, session_path};
use crate::kernel::project::{Message, derive_messages, state_of};
use crate::kernel::{CallId, Event};
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
        let _span = crate::trace_fn!("kernel::session::query::write_checkpoint");

        let checkpoint = Checkpoint::from_state(&self.state, goal, next_action);
        checkpoint::save(self.fs, &self.dir, &checkpoint)?;
        Ok(checkpoint)
    }

    /// Lê o checkpoint de fase, se existir.
    ///
    /// # Errors
    /// [`CheckpointError`] se o ficheiro existir mas não validar.
    pub fn read_checkpoint(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        let _span = crate::trace_fn!("kernel::session::query::read_checkpoint");

        checkpoint::load(self.fs, &self.dir)
    }

    /// Histórico visível ao modelo, derivado **só** do log (§42).
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn messages(&self) -> Result<Vec<Message>, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::messages");

        Ok(derive_messages(&self.log_events()?))
    }

    /// Verifica a invariante `Model-visible ⟺ logged` (§42) em runtime: o estado corrente tem de
    /// ser exatamente a projeção do log.
    ///
    /// # Errors
    /// [`SessionError::Invariant`] se o estado divergir do log.
    pub fn verify(&self) -> Result<(), SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::verify");

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
        let _span = crate::trace_fn!("kernel::session::query::fork");

        match self.fs.read(&session_path(&self.dir)) {
            Ok(bytes) => self.fs.write_atomic(&session_path(dst_dir), &bytes)?,
            Err(FsError::NotFound) => {}
            Err(other) => return Err(SessionError::Fs(other)),
        }
        Session::open_with_cap(self.fs, dst_dir, self.cost.caps().global)
    }

    /// Compacta o contexto pelo gatilho do kernel (E09-T07); determinístico e explícito.
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn compact_context(
        &self,
        budget: ContextBudget,
        mode: CompactionMode,
    ) -> Result<Option<Compaction>, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::compact_context");

        Ok(compact(&self.log_events()?, budget, mode))
    }

    /// Monta o **contexto efetivo** do turno (E09-T01/T07): `assemble` com o orçamento; com a
    /// compactação ligada, o prefixo antigo é substituído pelo digest (determinístico, o original
    /// continua endereçável no log). Sem nada a compactar, o `assemble` puro mantém-se.
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn context(
        &self,
        budget: ContextBudget,
        mode: CompactionMode,
    ) -> Result<Context, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::context");

        let events = self.log_events()?;
        Ok(match compact(&events, budget, mode) {
            Some(compaction) if !compaction.replacements.is_empty() => compaction.context,
            _ => assemble(&events, budget),
        })
    }

    /// Ficheiros alterados (do *diff*) registados no log, **relativos à raiz** do workspace
    /// (E09-T03). Só contam tools que mudam o disco (`is_file_change`) e cujo `ToolResult` teve
    /// sucesso — um pedido recusado não alterou nada.
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn changed_files(&self) -> Result<Vec<String>, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::changed_files");

        let mut pending: Vec<(CallId, Vec<String>)> = Vec::new();
        let mut files = Vec::new();
        for event in self.log_events()? {
            match event {
                Event::ToolCall { call, tool } if tool.name.is_file_change() => {
                    let paths = tool
                        .resolved_paths
                        .iter()
                        .map(|path| self.relative(path.as_str()))
                        .collect();
                    pending.push((call, paths));
                }
                Event::ToolResult { call, outcome, .. } => {
                    if let Some(index) = pending.iter().position(|(id, _)| *id == call) {
                        let (_, paths) = pending.remove(index);
                        if outcome.is_success() {
                            files.extend(paths);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(files)
    }

    /// Comandos registados no log (E06-T07), na ordem causal (E09-T03).
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn recorded_commands(&self) -> Result<Vec<CommandRecord>, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::recorded_commands");

        let mut commands = Vec::new();
        for event in self.log_events()? {
            if let Event::CommandRecorded { record } = event {
                commands.push(record);
            }
        }
        Ok(commands)
    }

    /// Caminho relativo à raiz do projeto (ou o absoluto, se estiver fora dela).
    fn relative(&self, path: &str) -> String {
        let _span = crate::trace_fn!("kernel::session::query::relative");

        Path::new(path).strip_prefix(&self.root).map_or_else(
            |_| path.to_string(),
            |relative| relative.to_string_lossy().into_owned(),
        )
    }

    /// Lê os eventos do log.
    pub(super) fn log_events(&self) -> Result<Vec<Event>, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::log_events");

        let records = read_records(self.fs, &session_path(&self.dir))?;
        Ok(records.into_iter().map(|record| record.event).collect())
    }
}
