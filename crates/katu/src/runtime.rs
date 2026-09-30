//! Runtime do agente: monta a sessão com a memória de primeira classe e as regras (E03-T03/T07).
//!
//! É o **ponto de composição** do loop real: descobre a raiz do projeto, abre o adaptador de
//! memória e **recusa arrancar** se a memória não estiver saudável (fail-closed, DF4). O loop de
//! turnos (provider) chega em E12; aqui ficam os caminhos §42 de recall e de escrita de nota.

use std::path::Path;

use katu_core::error::Error;
use katu_core::kernel::{
    CallContext, CallId, Dispatch, Event, MemoryWriteRequest, Message, Session, SessionError,
    SessionId, discover_root, memory_recall_use,
};
use katu_core::memory::{Anchor, Memory, MemoryError, NoteType, PreWriteReq, RecallReq};
use katu_core::ports::{Clock, Fs};
use katu_policy::{PolicyError, ResolvedPath, RuleSet};
use katu_tools::recall::RecallTool;
use katu_tools::write::WriteNoteTool;

use crate::memory::KnudgeMemory;

/// Regras do protocolo de memória, versionadas no repositório (dado, não código).
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

/// Falha ao montar ou operar o runtime.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RuntimeError {
    /// Falha da porta de memória.
    #[error("memória: {0}")]
    Memory(#[from] MemoryError),
    /// Falha da sessão (log, transição, custo ou política).
    #[error("sessão: {0}")]
    Session(#[from] SessionError),
    /// Vocabulário de política inválido.
    #[error("política: {0}")]
    Policy(#[from] PolicyError),
}

impl From<RuntimeError> for Error {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Memory(source) => Self::unavailable(source.to_string()),
            RuntimeError::Policy(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Session(source) => Self::internal(source.to_string()),
        }
    }
}

/// Runtime de uma sessão com memória real (in-process) e regras do protocolo.
pub(crate) struct Runtime<'a> {
    pub(crate) clock: &'a dyn Clock,
    pub(crate) session: Session<'a>,
    pub(crate) memory: KnudgeMemory,
    pub(crate) rules: RuleSet,
    pub(crate) cwd: ResolvedPath,
    pub(crate) calls: u64,
}

impl<'a> Runtime<'a> {
    /// Abre o projeto a partir de `start`, **recusando arrancar** sem memória saudável (DF4).
    ///
    /// # Errors
    /// [`RuntimeError`] se a memória, o layout de sessão ou as regras falharem.
    pub(crate) fn open(
        fs: &'a dyn Fs,
        clock: &'a dyn Clock,
        start: &Path,
        goal: &str,
    ) -> Result<Self, RuntimeError> {
        let root = discover_root(fs, start);
        let memory = KnudgeMemory::open(&root)?;
        memory.status()?;
        let mut session = Session::create(fs, &root, clock.now().as_millis(), goal)?;
        // O runtime é um agente a atuar: abre o turno antes de qualquer tool call (§42).
        session.apply(&Event::TurnStart { turn: 1 })?;
        let rules = RuleSet::from_toml(MEMORY_POLICY)?;
        let cwd = ResolvedPath::from_canonical(&root)?;
        Ok(Self {
            clock,
            session,
            memory,
            rules,
            cwd,
            calls: 0,
        })
    }

    /// Raiz do projeto vinculada (delegada ao log/sessão).
    pub(crate) fn root(&self) -> &Path {
        self.session.root()
    }

    /// Histórico visível ao modelo (projeção do log, §42).
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    pub(crate) fn messages(&self) -> Result<Vec<Message>, SessionError> {
        self.session.messages()
    }

    /// Turno corrente (o turno é aberto por [`Runtime::open`]).
    pub(crate) fn turn(&self) -> u32 {
        self.session.state().turn
    }

    /// Identificador estável da sessão (afinidade do provider).
    #[must_use]
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session.id().map(SessionId::as_str)
    }

    /// Loga a mensagem do utilizador.
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub(crate) fn record_user(&mut self, text: &str) -> Result<(), SessionError> {
        self.session.apply(&Event::UserMessage {
            text: text.to_string(),
        })
    }

    /// Loga a mensagem do assistente (resposta do modelo).
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub(crate) fn record_assistant(&mut self, text: &str) -> Result<(), SessionError> {
        self.session.apply(&Event::AssistantMessage {
            text: text.to_string(),
        })
    }

    /// Fecha o turno (o `state` fica consistente para `verify`).
    ///
    /// # Errors
    /// [`SessionError`] se o turno não corresponder ao aberto.
    pub(crate) fn record_turn_end(&mut self, turn: u32) -> Result<(), SessionError> {
        self.session.apply(&Event::TurnEnd { turn })
    }

    /// Sessão (log e estado) do runtime — usada pelos testes e pelo loop real (E12).
    #[cfg(test)]
    pub(crate) fn session(&self) -> &Session<'a> {
        &self.session
    }

    /// Consulta a memória pelo caminho §42 (logado antes de executar; policy-gated).
    ///
    /// # Errors
    /// [`RuntimeError`] se a transição, o custo ou a política falharem.
    pub(crate) fn recall(&mut self, query: &str, limit: usize) -> Result<Dispatch, RuntimeError> {
        let call = self.call("recall");
        let tool = RecallTool {
            memory: &self.memory,
            req: RecallReq {
                query: query.to_string(),
                limit,
            },
        };
        let use_ = memory_recall_use(&self.cwd);
        let now = self.clock.now().as_millis();
        self.session
            .tool_call(
                call,
                &use_,
                CallContext {
                    rules: &self.rules,
                    now_millis: now,
                    tool: &tool,
                },
            )
            .map_err(RuntimeError::from)
    }

    /// Regista uma nota: recall prévio (protocolo) + escrita pelo gate de E05 (ordem §42).
    ///
    /// # Errors
    /// [`RuntimeError`] se o recall, a transição, o custo ou a política falharem.
    pub(crate) fn remember(&mut self, req: &PreWriteReq) -> Result<Dispatch, RuntimeError> {
        self.recall(&req.statement, 5)?;
        let call = self.call("write");
        let tool = WriteNoteTool {
            memory: &self.memory,
            req: req.clone(),
        };
        let now = self.clock.now().as_millis();
        let request = MemoryWriteRequest {
            cwd: &self.cwd,
            req,
            memory: &self.memory,
            rules: &self.rules,
            now_millis: now,
            tool: &tool,
        };
        Ok(self.session.memory_write(call, request)?)
    }

    /// Constrói o `PreWriteReq` de uma nota simples (afirmação; tipo e âncora opcionais).
    pub(crate) fn note(statement: &str, note_type: NoteType, anchor: Option<&str>) -> PreWriteReq {
        PreWriteReq {
            statement: statement.to_string(),
            note_type,
            anchor: anchor.map(Anchor::new),
            body: String::new(),
        }
    }

    /// Identificador de chamada único e determinístico dentro da sessão.
    fn call(&mut self, kind: &str) -> CallId {
        let id = self.calls;
        self.calls = self.calls.saturating_add(1);
        CallId::new(format!("{kind}-{id}"))
    }
}

#[cfg(test)]
mod tests;
