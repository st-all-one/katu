//! Consulta e ciclo de vida derivado da sessão: histórico, invariante, checkpoint e *fork*.
//!
//! Métodos puros sobre o log/estado (não mutam o log); separados de `mod.rs` para manter os
//! ficheiros dentro do limite e por coesão.

use std::path::Path;

use super::snapshot::MAX_TAIL_BYTES;
use super::{Session, SessionError};
use crate::context::{
    AssembleOptions, Assembly, Compaction, CompactionMode, Context, ContextBudget, PrimeMode,
    assemble_all, compact,
};
use crate::diag::{Level, events};
use crate::feedback::CommandRecord;
use crate::kernel::checkpoint::{self, Checkpoint, CheckpointError};
use crate::kernel::log::{read_records, session_path};
use crate::kernel::project::{Message, derive_messages, state_of};
use crate::kernel::{CallId, Durability, Event, StateSnapshot};
use crate::ports::FsError;

impl Session<'_> {
    /// Define a política de durabilidade do log (ADR 0024); default [`Durability::Event`].
    ///
    /// Um turno já escrito em modo `event` está durável; mudar para `turn` a meio só afeta os
    /// eventos seguintes (e o `flush` do próximo `TurnEnd`).
    pub fn set_durability(&mut self, durability: Durability) {
        let _span = crate::fn_span!(
            Level::Trace,
            events::LOG_APPEND,
            "kernel::session::set_durability"
        );
        self.durability = durability;
        self.log.set_durability(durability);
    }

    /// Política de durabilidade em vigor. Só existe para os testes (ADR 0024).
    #[must_use]
    #[cfg(test)]
    pub const fn durability(&self) -> Durability {
        self.durability
    }

    /// `true` se há eventos escritos por sincronizar (modo `turn`). Só existe para os testes.
    #[must_use]
    #[cfg(test)]
    pub const fn is_dirty(&self) -> bool {
        self.log.is_dirty()
    }

    /// Fecha a barreira pendente do log (no-op no modo `event`).
    ///
    /// # Errors
    /// [`SessionError::Log`] se a barreira falhar.
    pub fn flush(&mut self) -> Result<(), SessionError> {
        let _span = crate::fn_span!(Level::Trace, events::LOG_APPEND, "kernel::session::flush");
        self.log.flush()?;
        Ok(())
    }

    /// Fronteira de snapshot: transição de fase (contrato) ou fim de turno com a cauda cheia (Q-15).
    ///
    /// O snapshot é uma **otimização reconstruível**: uma falha ao gravar não invalida o turno (o
    /// log é a fonte da verdade), pelo que o erro só se regista no diagnóstico.
    pub(super) fn maybe_snapshot(&mut self, event: &Event) {
        let _span = crate::trace_fn!("kernel::session::maybe_snapshot");

        let phase = matches!(event, Event::PhaseTransition { .. });
        let full_tail =
            matches!(event, Event::TurnEnd { .. }) && self.tail_bytes() >= MAX_TAIL_BYTES;
        if !(phase || full_tail) {
            return;
        }
        match self.snapshot_now() {
            Ok(_) => crate::event!(
                Level::Debug,
                events::SESSION_SNAPSHOT,
                "ok" => true,
                "tail_bytes" => self.tail_bytes()
            ),
            Err(_) => crate::event!(Level::Warn, events::SESSION_SNAPSHOT, "ok" => false),
        }
    }

    /// Bytes de log desde o último snapshot (o que a retomada teria de reler).
    #[must_use]
    pub fn tail_bytes(&self) -> u64 {
        let _span = crate::trace_fn!("kernel::session::tail_bytes");

        self.log.offset().saturating_sub(self.snapshot_offset)
    }

    /// Grava o snapshot e **regista** a fronteira (teto da cauda, Q-15).
    pub(super) fn snapshot_now(&mut self) -> Result<StateSnapshot, SessionError> {
        let _span = crate::trace_fn!("kernel::session::snapshot_now");

        let snapshot = self.write_snapshot()?;
        self.snapshot_offset = snapshot.offset;
        Ok(snapshot)
    }

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

        Ok(self
            .assemble(budget, AssembleOptions::new(PrimeMode::Compact, mode))?
            .context)
    }

    /// Monta contexto **e** compactação numa só passagem (S-01), com as opções do chamador
    /// (Q-02b/Q-03/Q-04): política de seleção, objetivo e secção de estado.
    ///
    /// # Errors
    /// [`SessionError::Log`] se o log estiver corrompido.
    pub fn assemble(
        &self,
        budget: ContextBudget,
        options: AssembleOptions<'_>,
    ) -> Result<Assembly, SessionError> {
        let _span = crate::trace_fn!("kernel::session::query::assemble");

        Ok(assemble_all(&self.log_events()?, budget, options))
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
