//! Sessão e loop mínimo (E04-T08): consome eventos, aplica transições e grava no log.
//!
//! Ordem §42 numa tool call: `ToolCall` **logado antes** de executar → política → efeito →
//! `ToolResult` logado. Uma recusa de `step` (ou de orçamento, E04-T07) **não** altera o estado
//! nem o log (fail-closed).

use std::path::{Path, PathBuf};

use super::budget::{Budget, BudgetCap, BudgetGate};
use super::cost::{CostCaps, CostGovernor, cost_charge_for};
use super::event::{CallId, Event};
use super::log::{Log, read_records_from, read_records_with_len, session_path};
use super::memory_gate::{MemoryWriteRequest, enforce_memory_write, memory_write_use};
use super::pipeline::{Dispatch, dispatch};
use super::state::State;
use super::step::step;
use crate::diag::{Level, events};
use crate::ports::Fs;
use crate::verify::VerificationReport;
use katu_policy::{ResolvedPath, ToolUse};

mod approval;
mod context;
mod error;
mod identity;
mod query;
mod resume;
mod snapshot;

pub use context::CallContext;
use context::log_outcome;
pub use error::SessionError;
pub use identity::{SessionId, SessionMeta, audit_dir, discover_root, katu_dir};
pub use snapshot::StateSnapshot;

/// Sessão append-only: o log é a fonte da verdade e o estado é a sua projeção.
pub struct Session<'a> {
    fs: &'a dyn Fs,
    dir: PathBuf,
    root: PathBuf,
    meta: Option<SessionMeta>,
    log: Log<'a>,
    state: State,
    cost: CostGovernor,
}

impl<'a> Session<'a> {
    /// Abre (ou cria) a sessão no diretório, sem tetos de orçamento.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido ou contiver uma transição inválida.
    pub fn open(fs: &'a dyn Fs, dir: &Path) -> Result<Self, SessionError> {
        let _span = crate::trace_fn!("kernel::session::open");

        Self::open_with_cap(fs, dir, BudgetCap::NONE)
    }

    /// Abre a sessão com tetos de orçamento, retomando uso e estado a partir do log.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido ou contiver uma transição inválida.
    pub fn open_with_cap(fs: &'a dyn Fs, dir: &Path, cap: BudgetCap) -> Result<Self, SessionError> {
        let _span = crate::trace_fn!("kernel::session::open_with_cap");

        Self::open_with_cost(
            fs,
            dir,
            CostCaps {
                global: cap,
                ..CostCaps::default()
            },
        )
    }

    /// Abre a sessão com as camadas do cost governor (E09-T06), retomando uso e estado do log.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido ou contiver uma transição inválida.
    pub fn open_with_cost(
        fs: &'a dyn Fs,
        dir: &Path,
        caps: CostCaps,
    ) -> Result<Self, SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::STORE_LOAD,
            "kernel::session::open_with_cost"
        );
        let path = session_path(dir);
        let meta = identity::load_meta(fs, dir);
        let root = meta
            .as_ref()
            .map_or_else(|| dir.to_path_buf(), |meta| PathBuf::from(&meta.root));
        let snapshot = snapshot::load(fs, dir);
        let snapshot_seq = snapshot.as_ref().map_or(0, |snapshot| snapshot.seq);
        // Só lê a cauda depois do snapshot (ADR 0008); qualquer desalinhamento cai no replay total.
        let tail = snapshot.as_ref().and_then(|snapshot| {
            (snapshot.offset > 0)
                .then(|| {
                    read_records_from(fs, &path, snapshot.offset, snapshot.seq.saturating_add(1))
                })
                .and_then(Result::ok)
        });
        let (mut state, mut cost, records, total) =
            if let (Some(snapshot), Some((records, len))) = (snapshot, tail) {
                let cost = CostGovernor::with_history(
                    caps,
                    snapshot.budget,
                    snapshot.per_tool,
                    snapshot.history,
                );
                (snapshot.state, cost, records, len)
            } else {
                let (records, len) = read_records_with_len(fs, &path)?;
                (
                    State::initial(),
                    CostGovernor::new(caps, Budget::ZERO),
                    records,
                    len,
                )
            };
        for record in &records {
            state = step(&state, &record.event)?;
            if let Some(charge) = cost_charge_for(&record.event) {
                cost.commit(&charge);
            }
        }
        let last_seq = records.last().map_or(snapshot_seq, |record| record.seq);
        let log = Log::resume(fs, dir, last_seq, u64::try_from(total).unwrap_or(u64::MAX));
        Ok(Self {
            fs,
            dir: dir.to_path_buf(),
            root,
            meta,
            log,
            state,
            cost,
        })
    }

    /// Estado corrente (projeção do log).
    #[must_use]
    pub fn state(&self) -> &State {
        let _span = crate::trace_fn!("kernel::session::state");

        &self.state
    }

    /// Cost governor (E09-T06): único dono dos tetos.
    #[must_use]
    pub fn cost(&self) -> &CostGovernor {
        let _span = crate::trace_fn!("kernel::session::cost");

        &self.cost
    }

    /// Portão global/tarefa (único dono do teto de contexto).
    #[must_use]
    pub fn budget(&self) -> BudgetGate {
        let _span = crate::trace_fn!("kernel::session::budget");

        self.cost.global()
    }

    /// Caminho do ficheiro de log da sessão.
    #[must_use]
    pub fn log_path(&self) -> &Path {
        let _span = crate::trace_fn!("kernel::session::log_path");

        self.log.path()
    }

    /// Aplica um evento: valida transição **e custo** antes de gravar.
    ///
    /// # Errors
    /// [`SessionError::Refusal`] se a transição for inválida; [`SessionError::Cost`] se um teto for
    /// atingido; [`SessionError::Log`] se a gravação falhar. Em qualquer caso o estado fica
    /// inalterado.
    pub fn apply(&mut self, event: &Event) -> Result<(), SessionError> {
        let _span = crate::trace_fn!("kernel::session::apply");

        self.apply_at(event, None)
    }

    /// Aplica um evento com o instante (para as camadas temporais do governor).
    fn apply_at(&mut self, event: &Event, now_millis: Option<u64>) -> Result<(), SessionError> {
        let _span = crate::fn_span!(Level::Trace, events::KERNEL_TRANSITION, "kernel::session::apply_at", "event" => event.kind());
        let charge = cost_charge_for(event).map(|charge| match now_millis {
            Some(now) => charge.at(now),
            None => charge,
        });
        let next = step(&self.state, event)?;
        if let Some(charge) = &charge {
            self.cost.check(charge)?;
        }
        self.log.append(event)?;
        if let Some(charge) = &charge {
            self.cost.commit(charge);
        }
        self.state = next;
        if matches!(event, Event::PhaseTransition { .. }) && self.write_snapshot().is_err() {
            crate::event!(Level::Warn, events::SESSION_SNAPSHOT, "ok" => false);
        }
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
        let _span = crate::fn_span!(
            Level::Trace,
            events::TOOL_CALL,
            "kernel::session::tool_call"
        );
        self.apply_at(
            &Event::ToolCall {
                call: call.clone(),
                tool: use_.clone(),
            },
            Some(context.now_millis),
        )?;
        let outcome = dispatch(
            &self.state,
            use_,
            context.rules,
            context.now_millis,
            context.tool,
        )?;
        let result = outcome.outcome();
        log_outcome(&result);
        self.apply(&Event::ToolResult {
            call,
            outcome: result,
        })?;
        Ok(outcome)
    }

    /// Define a raiz do workspace (E07-T05): a partir daqui a política concede ler/escrever sob a
    /// raiz e exige autorização fora dela. O evento fica no log (auditável).
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub fn set_workspace(&mut self, root: &ResolvedPath) -> Result<(), SessionError> {
        let _span = crate::trace_fn!("kernel::session::set_workspace");

        self.apply(&Event::WorkspaceSet { root: root.clone() })
    }

    /// Regista o relatório do gate de verificação (E09-T03): exigido para transitar para
    /// `Phase::Verified`. O evento fica no log (auditável).
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub fn record_verification(&mut self, report: &VerificationReport) -> Result<(), SessionError> {
        let _span = crate::trace_fn!("kernel::session::record_verification");

        self.apply(&Event::VerificationRecorded {
            report: report.clone(),
        })
    }

    /// Executa uma **escrita de memória** pela ordem §42, com o gate de E05: loga o pedido, corre
    /// `pre_write` → capacidade → política → efeito, e loga o resultado.
    ///
    /// # Errors
    /// [`SessionError`] se a chamada não puder ser logada, a política recusar ou o `pre_write`
    /// falhar.
    pub fn memory_write(
        &mut self,
        call: CallId,
        request: MemoryWriteRequest<'_>,
    ) -> Result<Dispatch, SessionError> {
        let _span = crate::fn_span!(
            Level::Trace,
            events::MEMORY_WRITE,
            "kernel::session::memory_write"
        );
        self.apply(&Event::ToolCall {
            call: call.clone(),
            tool: memory_write_use(request.cwd),
        })?;
        let dispatch = enforce_memory_write(&self.state, request)?;
        let result = dispatch.outcome();
        log_outcome(&result);
        self.apply(&Event::ToolResult {
            call,
            outcome: result,
        })?;
        Ok(dispatch)
    }
}
#[cfg(test)]
mod tests;
