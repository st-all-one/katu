//! Runtime do agente: monta a sessão com a memória de primeira classe e as regras (E03-T03/T07).
//!
//! É o **ponto de composição** do loop real: descobre a raiz do projeto, abre o adaptador de
//! memória e **recusa arrancar** se a memória não estiver saudável (fail-closed, DF4). O loop de
//! turnos (provider) chega em E12; aqui ficam os caminhos §42 de recall e de escrita de nota.

use std::path::Path;

use katu_core::context::{CompactionMode, ContextBudget};
use katu_core::diag::{Level, events};
#[cfg(test)]
use katu_core::kernel::Message;
use katu_core::kernel::{
    CallId, Durability, Event, Session, SessionError, SessionId, discover_root,
};
use katu_core::memory::Memory;
use katu_core::plan::Plan;
use katu_core::ports::{Clock, Fs};
use katu_core::skill::{Skill, catalog as skill_catalog};
use katu_policy::{PolicyError, ResolvedPath, RuleSet};

use crate::defaults;
use crate::memory::KnudgeMemory;
use crate::scope;

mod checkpoint;
mod context;
mod control;
mod error;
mod memory;
mod plan_mode;
mod skills;
mod transcript;
mod verify;

pub(crate) use error::RuntimeError;

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
#[cfg_attr(
    not(feature = "profile"),
    allow(
        unused_variables,
        reason = "o macro no-op ignora os campos (custo zero); o audit é barato e determinístico"
    )
)]
fn load_rules(now_millis: u64) -> Result<(RuleSet, Vec<String>), PolicyError> {
    let _span = katu_core::fn_span!(Level::Debug, events::POLICY_LOAD, "runtime::load_rules");
    let mut rules = RuleSet::from_toml(MEMORY_POLICY)?;
    rules
        .rules
        .extend(RuleSet::from_toml(CONTAINMENT_POLICY)?.rules);
    // E02-T04 instrumentado **pelo chamador** (firewall): o binário audita o conjunto carregado.
    let report = {
        let _span = katu_core::fn_span!(Level::Debug, events::POLICY_AUDIT, "policy::audit");
        katu_policy::audit(&rules, now_millis)
    };
    katu_core::event!(
        Level::Debug,
        events::POLICY_AUDIT,
        "enforced" => report.enforced.len(),
        "advisory" => report.advisory.len(),
        "issues" => report.issues.len(),
    );
    // A secção `estado` (Q-04) mostra as regras que **travam** de facto: as ativas no arranque.
    let enforced = report
        .enforced
        .iter()
        .filter(|summary| summary.activity == katu_policy::Activity::Active)
        .map(|summary| summary.id.as_str().to_string())
        .collect();
    Ok((rules, enforced))
}

/// Id da sessão mais recente do projeto (ordem temporal `(created_ms, id)`).
fn latest_session(fs: &dyn Fs, root: &Path) -> Result<SessionId, RuntimeError> {
    let _span = katu_core::trace_fn!("runtime::latest_session");

    Session::list(fs, root)?
        .pop()
        .map(|meta| meta.id)
        .ok_or_else(|| RuntimeError::Resume("nenhuma sessão para retomar".to_string()))
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
    /// Modo de planeamento ligado (E20-T11); injeta a regra “escrita só sob `.katu/`”.
    pub(crate) plan_mode: bool,
    /// Instruções do projeto (`AGENTS.md`), se existirem (E20-T13).
    instructions: Option<String>,
    /// Skills descobertas no arranque (E20-T13).
    skills: Vec<Skill>,
    goal: String,
    budget: ContextBudget,
    compaction: CompactionMode,
    /// Política do contexto do turno (Q-02b/Q-03/Q-04): seleção, parâmetros e secção de estado.
    policy: context::ContextPolicy,
    /// Teto de passos do turno corrente (entra na secção `estado`).
    max_steps: u32,
    /// Secção `estado` já registada no log e usada no prime deste turno.
    state_text: Option<String>,
    /// Ids das regras `Enforced` ativas (para a secção `estado`).
    enforced: Vec<String>,
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
        let _span = katu_core::trace_fn!("runtime::open");

        let root = discover_root(fs, start);
        let session = Session::create(fs, &root, clock.now().as_millis(), goal)?;
        Self::assemble(fs, clock, goal, session)
    }

    /// Retoma a sessão mais recente do projeto (ou a indicada por `id`) e abre o próximo turno.
    ///
    /// Um turno deixado **aberto** por um processo morto a meio é fechado de forma determinística
    /// (`TurnEnd`) antes de abrir o seguinte — a retomada nunca reabre um turno parcial.
    ///
    /// # Errors
    /// [`RuntimeError::Resume`] se não houver sessão, o id for inválido ou desconhecido;
    /// [`RuntimeError`] se a memória, o layout ou as regras falharem.
    pub(crate) fn resume(
        fs: &'a dyn Fs,
        clock: &'a dyn Clock,
        start: &Path,
        goal: &str,
        id: Option<&str>,
    ) -> Result<Self, RuntimeError> {
        let _span = katu_core::trace_fn!("runtime::resume");

        let root = discover_root(fs, start);
        let id = match id {
            Some(id) => SessionId::parse(id)
                .ok_or_else(|| RuntimeError::Resume(format!("id de sessão inválido: {id}")))?,
            None => latest_session(fs, &root)?,
        };
        let session = Session::resume(fs, &root, &id)?;
        Self::assemble(fs, clock, goal, session)
    }

    /// Monta o runtime sobre uma sessão já aberta (criação ou retomada) — ponto único de composição.
    fn assemble(
        fs: &'a dyn Fs,
        clock: &'a dyn Clock,
        goal: &str,
        mut session: Session<'a>,
    ) -> Result<Self, RuntimeError> {
        let _span = katu_core::trace_fn!("runtime::assemble");

        let root = session.root().to_path_buf();
        let plan = scope::load(fs, &root)?;
        let memory = KnudgeMemory::open(&root)?;
        memory.status()?;
        let cwd = ResolvedPath::from_canonical(&root)?;
        let (rules, enforced) = load_rules(clock.now().as_millis())?;
        // Q-04/Q-02b: as duas opções são **dados** do projeto (config fechada), lidas uma vez.
        let defaults = defaults::from_root(&root);
        // ADR 0024 (P-01): a política de durabilidade é dado do projeto; o default passou a ser
        // `turn` (*group commit*, decisão do dono). Um valor desconhecido cai no default.
        let durability = defaults
            .durability
            .as_deref()
            .and_then(Durability::parse)
            .unwrap_or(Durability::Turn);
        session.set_durability(durability);
        let instructions = skills::read_instructions(fs, &root);
        let skills = skills::load_skills(fs, &root);
        // O runtime é um agente a atuar: define o workspace (destranca o normal dentro da raiz e
        // exige aprovação fora) antes de qualquer tool call (§42).
        session.set_workspace(&cwd)?;
        // L-Q6: só um processo escreve no mesmo log de cada vez. O lock é do turno (não da
        // sessão): entre turnos a UI está ociosa e outro processo pode retomar.
        session.acquire_turn_lock(clock.now().as_millis())?;
        if session.state().turn_open {
            // L-Q1: um turno aberto por um processo morto fecha com as calls pendentes
            // reconciliadas — o próximo pedido nunca leva um par desalinhado.
            session.reconcile_pending()?;
            let open = session.state().turn;
            session.apply(&Event::TurnEnd { turn: open })?;
        }
        let next = session.state().turn.saturating_add(1);
        session.apply(&Event::TurnStart { turn: next })?;
        let goal = session
            .meta()
            .map_or_else(|| goal.to_string(), |meta| meta.goal.clone());
        // O prompt de sistema tem de ser reconstruível do log (E04/Q-16): regista-se o **texto
        // exato** do contexto do projeto, não a fonte crua nem um hash — uma retomada com o
        // `AGENTS.md` alterado tem de reproduzir o prompt que foi enviado.
        let skills_text = skill_catalog(&skills, &goal, &root);
        session.apply(&Event::ProjectContext {
            agents: instructions.clone(),
            skills: (!skills_text.is_empty()).then_some(skills_text),
        })?;
        Ok(Self {
            clock,
            session,
            memory,
            rules,
            cwd,
            calls: 0,
            plan,
            plan_mode: false,
            instructions,
            skills,
            goal,
            budget: DEFAULT_CONTEXT_BUDGET,
            compaction: CompactionMode::Disabled,
            policy: context::ContextPolicy::from_config(
                defaults.context_selection.as_deref(),
                defaults.prompt_state,
            ),
            max_steps: 0,
            state_text: None,
            enforced,
        })
    }

    /// Abre o próximo turno (multi-turno, E10): o turno anterior tem de estar fechado.
    ///
    /// # Errors
    /// [`SessionError`] se já houver um turno aberto ou a transição for recusada.
    pub(crate) fn begin_turn(&mut self) -> Result<(), SessionError> {
        let _span = katu_core::trace_fn!("runtime::begin_turn");

        self.session
            .acquire_turn_lock(self.clock.now().as_millis())?;
        let next = self.session.state().turn.saturating_add(1);
        self.session.apply(&Event::TurnStart { turn: next })
    }

    /// Raiz do projeto vinculada (delegada ao log/sessão).
    pub(crate) fn root(&self) -> &Path {
        let _span = katu_core::trace_fn!("runtime::root");

        self.session.root()
    }

    /// Fase corrente do kernel (E10-T06).
    #[must_use]
    pub(crate) fn phase(&self) -> katu_policy::Phase {
        let _span = katu_core::trace_fn!("runtime::phase");

        self.session.state().phase
    }

    /// Plano carregado no arranque (E09-T04); `None` mantém a tool `plan` indisponível.
    #[must_use]
    pub(crate) fn plan(&self) -> Option<&Plan> {
        let _span = katu_core::trace_fn!("runtime::plan");

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
        let _span = katu_core::trace_fn!("runtime::turn");

        self.session.state().turn
    }

    /// Identificador estável da sessão (afinidade do provider).
    #[must_use]
    pub(crate) fn session_id(&self) -> Option<&str> {
        let _span = katu_core::trace_fn!("runtime::session_id");

        self.session.id().map(SessionId::as_str)
    }

    /// Loga a mensagem do utilizador.
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub(crate) fn record_user(&mut self, text: &str) -> Result<(), SessionError> {
        let _span = katu_core::trace_fn!("runtime::record_user");

        self.session.apply(&Event::UserMessage {
            text: text.to_string(),
        })
    }

    /// Loga a mensagem do assistente (resposta do modelo).
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub(crate) fn record_assistant(&mut self, text: &str) -> Result<(), SessionError> {
        let _span = katu_core::trace_fn!("runtime::record_assistant");

        self.session.apply(&Event::AssistantMessage {
            text: text.to_string(),
        })
    }

    /// Fecha o turno (o `state` fica consistente para `verify`).
    ///
    /// # Errors
    /// [`SessionError`] se o turno não corresponder ao aberto.
    pub(crate) fn record_turn_end(&mut self, turn: u32) -> Result<(), SessionError> {
        let _span = katu_core::trace_fn!("runtime::record_turn_end");

        // L-Q1/L-S1: **único** ponto de fecho — reconcilia as calls pendentes (erro, cancel, teto
        // ou morte a meio) antes de fechar o turno; um turno normal não acrescenta eventos.
        self.session.reconcile_pending()?;
        self.session.apply(&Event::TurnEnd { turn })?;
        self.session.release_turn_lock()
    }

    /// Sessão (log e estado) do runtime — usada pelos testes e pelo loop real (E12).
    #[cfg(test)]
    pub(crate) fn session(&self) -> &Session<'a> {
        &self.session
    }

    /// Sessão mutável (só testes: montar um turno a meio, ex.: uma call pendente para L-Q1).
    #[cfg(test)]
    pub(crate) fn session_mut(&mut self) -> &mut Session<'a> {
        &mut self.session
    }

    /// Identificador de chamada único e determinístico dentro da sessão.
    fn call(&mut self, kind: &str) -> CallId {
        let _span = katu_core::trace_fn!("runtime::call");

        let id = self.calls;
        self.calls = self.calls.saturating_add(1);
        CallId::new(format!("{kind}-{id}"))
    }
}

#[cfg(test)]
mod tests;
