//! Runtime do agente: monta a sessão com a memória de primeira classe e as regras (E03-T03/T07).
//!
//! É o **ponto de composição** do loop real: descobre a raiz do projeto, abre o adaptador de
//! memória e **recusa arrancar** se a memória não estiver saudável (fail-closed, DF4). O loop de
//! turnos (provider) chega em E12; aqui ficam os caminhos §42 de recall e de escrita de nota.

use std::path::Path;

use katu_core::context::{CompactionMode, ContextBudget};
use katu_core::error::Error;
#[cfg(test)]
use katu_core::kernel::Message;
use katu_core::kernel::{
    CallContext, CallId, Dispatch, Event, MemoryWriteRequest, Session, SessionError, SessionId,
    discover_root, memory_recall_use,
};
use katu_core::memory::{Anchor, Memory, MemoryError, NoteType, PreWriteReq, RecallReq};
use katu_core::plan::Plan;
use katu_core::ports::{Clock, Fs};
use katu_policy::{PolicyError, ResolvedPath, RuleSet};
use katu_tools::recall::RecallTool;
use katu_tools::write::WriteNoteTool;

use crate::memory::KnudgeMemory;
use crate::scope::{self, ScopeError};

mod context;
mod verify;

/// Regras do protocolo de memória, versionadas no repositório (dado, não código).
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

/// Regras de contenção **soft** (E07-T05): sensíveis e fora do workspace.
const CONTAINMENT_POLICY: &str = include_str!("../../../policy/containment.toml");

/// Orçamento de contexto do turno — o **único dono do teto** (E09-T01/T07).
pub(crate) const DEFAULT_CONTEXT_BUDGET: ContextBudget = ContextBudget {
    raw_min: 4096,
    summary_max: 1024,
};

/// Parâmetros do gate de verificação (struct evita booleano nu em parâmetros).
#[derive(Debug, Clone, Copy)]
pub(crate) struct VerifyRequest {
    /// Cobertura mínima exigida (pontos base).
    pub coverage_floor_bps: u16,
    /// `--strict`: promove `Warn` a `Block`.
    pub strict: bool,
}

/// Carrega as regras do protocolo de memória **e** as de contenção (DF3: dado versionado).
fn load_rules() -> Result<RuleSet, PolicyError> {
    let mut rules = RuleSet::from_toml(MEMORY_POLICY)?;
    rules
        .rules
        .extend(RuleSet::from_toml(CONTAINMENT_POLICY)?.rules);
    Ok(rules)
}

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
    /// Artefacto de plano (`scope_contract`/`feature_list`) inválido (E09-T04).
    #[error("escopo: {0}")]
    Scope(#[from] ScopeError),
    /// O gate de verificação (E09-T03) não pôde correr (falta o escopo).
    #[error("verificação: {0}")]
    Verification(String),
}

impl From<RuntimeError> for Error {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Memory(source) => Self::unavailable(source.to_string()),
            RuntimeError::Policy(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Scope(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Verification(message) => Self::invalid_input(message),
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
    pub(crate) plan: Option<Plan>,
    budget: ContextBudget,
    compaction: CompactionMode,
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
        let plan = scope::load(fs, &root)?;
        let memory = KnudgeMemory::open(&root)?;
        memory.status()?;
        let cwd = ResolvedPath::from_canonical(&root)?;
        let rules = load_rules()?;
        let mut session = Session::create(fs, &root, clock.now().as_millis(), goal)?;
        // O runtime é um agente a atuar: define o workspace (destranca o normal dentro da raiz e
        // exige aprovação fora) e abre o turno antes de qualquer tool call (§42).
        session.set_workspace(&cwd)?;
        session.apply(&Event::TurnStart { turn: 1 })?;
        Ok(Self {
            clock,
            session,
            memory,
            rules,
            cwd,
            calls: 0,
            plan,
            budget: DEFAULT_CONTEXT_BUDGET,
            compaction: CompactionMode::Disabled,
        })
    }

    /// Abre o próximo turno (multi-turno, E10): o turno anterior tem de estar fechado.
    ///
    /// # Errors
    /// [`SessionError`] se já houver um turno aberto ou a transição for recusada.
    pub(crate) fn begin_turn(&mut self) -> Result<(), SessionError> {
        let next = self.session.state().turn.saturating_add(1);
        self.session.apply(&Event::TurnStart { turn: next })
    }

    /// Raiz do projeto vinculada (delegada ao log/sessão).
    pub(crate) fn root(&self) -> &Path {
        self.session.root()
    }

    /// Fase corrente do kernel (E10-T06).
    #[must_use]
    pub(crate) fn phase(&self) -> katu_policy::Phase {
        self.session.state().phase
    }

    /// Plano carregado no arranque (E09-T04); `None` mantém a tool `plan` indisponível.
    #[must_use]
    pub(crate) fn plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// Histórico visível ao modelo (projeção do log, §42) — usado pelos testes do loop.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    #[cfg(test)]
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
