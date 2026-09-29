//! Sessão e loop mínimo (E04-T08): consome eventos, aplica transições e grava no log.
//!
//! Ordem §42 numa tool call: `ToolCall` **logado antes** de executar → política → efeito →
//! `ToolResult` logado. Uma recusa de `step` (ou de orçamento, E04-T07) **não** altera o estado
//! nem o log (fail-closed).

use std::path::{Path, PathBuf};

use super::budget::{Budget, BudgetCap, BudgetGate, BudgetRefusal, charge_for};
use super::checkpoint::{self, Checkpoint, CheckpointError};
use super::event::{CallId, Event};
use super::log::{Log, LogError, read_records, session_path};
use super::pipeline::{Dispatch, Tool, dispatch};
use super::project::{Message, derive_messages, state_of};
use super::state::{Refusal, State};
use super::step::step;
use crate::ports::{Fs, FsError};
use katu_policy::{PolicyError, RuleSet, ToolUse};

/// Erro de uma operação de sessão.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// Falha no log.
    #[error("log: {0}")]
    Log(#[from] LogError),
    /// Transição recusada (fail-closed).
    #[error("recusa: {0}")]
    Refusal(#[from] Refusal),
    /// Vocabulário de política inválido.
    #[error("política: {0}")]
    Policy(#[from] PolicyError),
    /// Teto de orçamento atingido (E04-T07).
    #[error("orçamento: {0}")]
    Budget(#[from] BudgetRefusal),
    /// Falha no checkpoint de fase.
    #[error("checkpoint: {0}")]
    Checkpoint(#[from] CheckpointError),
    /// Falha de sistema de ficheiros.
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    /// Invariante `Model-visible ⟺ logged` violada (§42).
    #[error("invariante: {0}")]
    Invariant(String),
}

/// Contexto de execução de uma tool call (evita uma assinatura com demasiados argumentos).
#[derive(Clone, Copy)]
pub struct CallContext<'a> {
    /// Regras de política a avaliar.
    pub rules: &'a RuleSet,
    /// Instante corrente (ms desde a época), injetado pelo clock do kernel.
    pub now_millis: u64,
    /// Tool a executar se a política permitir.
    pub tool: &'a dyn Tool,
}

/// Sessão append-only: o log é a fonte da verdade e o estado é a sua projeção.
pub struct Session<'a> {
    fs: &'a dyn Fs,
    dir: PathBuf,
    log: Log<'a>,
    state: State,
    budget: BudgetGate,
}

impl<'a> Session<'a> {
    /// Abre (ou cria) a sessão no diretório, sem tetos de orçamento.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido ou contiver uma transição inválida.
    pub fn open(fs: &'a dyn Fs, dir: &Path) -> Result<Self, SessionError> {
        Self::open_with_cap(fs, dir, BudgetCap::NONE)
    }

    /// Abre a sessão com tetos de orçamento, retomando uso e estado a partir do log.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido ou contiver uma transição inválida.
    pub fn open_with_cap(fs: &'a dyn Fs, dir: &Path, cap: BudgetCap) -> Result<Self, SessionError> {
        let records = read_records(fs, &session_path(dir))?;
        let events: Vec<Event> = records.into_iter().map(|record| record.event).collect();
        let state = state_of(&events)?;
        let budget = BudgetGate::resume(cap, Budget::from_events(&events));
        let log = Log::open(fs, dir)?;
        Ok(Self {
            fs,
            dir: dir.to_path_buf(),
            log,
            state,
            budget,
        })
    }

    /// Estado corrente (projeção do log).
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Portão de orçamento (único dono do teto de contexto).
    #[must_use]
    pub fn budget(&self) -> BudgetGate {
        self.budget
    }

    /// Caminho do ficheiro de log da sessão.
    #[must_use]
    pub fn log_path(&self) -> &Path {
        self.log.path()
    }

    /// Aplica um evento: valida transição **e orçamento** antes de gravar.
    ///
    /// # Errors
    /// [`SessionError::Refusal`] se a transição for inválida; [`SessionError::Budget`] se um teto
    /// for atingido; [`SessionError::Log`] se a gravação falhar. Em qualquer caso o estado fica
    /// inalterado.
    pub fn apply(&mut self, event: &Event) -> Result<(), SessionError> {
        let charge = charge_for(event);
        let next = step(&self.state, event)?;
        if let Some(charge) = charge {
            self.budget.check(charge)?;
        }
        self.log.append(event)?;
        if let Some(charge) = charge {
            self.budget.commit(charge);
        }
        self.state = next;
        Ok(())
    }

    /// Executa uma tool call pela ordem §42: loga o pedido, avalia a política, executa, loga o
    /// resultado. Um teto de orçamento recusado impede o efeito.
    ///
    /// # Errors
    /// [`SessionError`] se a chamada não puder ser logada, o orçamento recusar ou a política for
    /// inválida.
    pub fn tool_call(
        &mut self,
        call: CallId,
        use_: &ToolUse,
        context: CallContext<'_>,
    ) -> Result<Dispatch, SessionError> {
        self.apply(&Event::ToolCall {
            call: call.clone(),
            tool: use_.clone(),
        })?;
        let outcome = dispatch(
            &self.state,
            use_,
            context.rules,
            context.now_millis,
            context.tool,
        )?;
        self.apply(&Event::ToolResult {
            call,
            outcome: outcome.outcome(),
        })?;
        Ok(outcome)
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
        Session::open_with_cap(self.fs, dst_dir, self.budget.cap())
    }

    /// Lê os eventos do log.
    fn log_events(&self) -> Result<Vec<Event>, SessionError> {
        let records = read_records(self.fs, &session_path(&self.dir))?;
        Ok(records.into_iter().map(|record| record.event).collect())
    }
}

#[cfg(test)]
mod tests;
