//! Contexto efetivo do turno e compactação (E09-T01/T07).

use katu_core::context::{
    AssembleOptions, Assembly, Compaction, CompactionMode, Context, ContextBudget, PrimeMode,
    SelectionParams, SelectionPolicy, StateView, state_section,
};
use katu_core::kernel::{Event, SessionError};

use super::Runtime;
use crate::defaults::Defaults;

/// Teto cru do contexto: dado do projeto (`behavior.context_budget`) ou default do binário.
pub(crate) fn context_budget(defaults: &Defaults) -> ContextBudget {
    let _span = katu_core::trace_fn!("runtime::context_budget");

    ContextBudget {
        raw_min: defaults
            .context_budget
            .unwrap_or(super::DEFAULT_CONTEXT_BUDGET.raw_min),
        summary_max: super::DEFAULT_CONTEXT_BUDGET.summary_max,
    }
}

/// Política do contexto do turno (Q-02b/Q-03/Q-04) — **dados** do projeto, não interruptores soltos.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextPolicy {
    /// Política de seleção do contexto; `suffix` é o default histórico.
    pub(crate) selection: SelectionPolicy,
    /// Parâmetros da seleção (dados versionados).
    pub(crate) params: SelectionParams,
    /// Incluir a secção `estado` no prime (Q-04) — **ligado** por omissão (decisão do dono).
    pub(crate) prompt_state: bool,
}

impl ContextPolicy {
    /// Lê a política da config fechada do projeto (valor inválido ⇒ default, nunca invenção).
    ///
    /// O `prompt_state` chega como `Option<bool>` da config: ausente ⇒ **ligado** (decisão do dono).
    /// A seleção ausente/ inválida ⇒ [`SelectionPolicy::Suffix`] (o default histórico, seguro; a
    /// `utility` só entra por config explícita — Q-02b/Q-03).
    #[must_use]
    pub(crate) fn from_config(selection: Option<&str>, prompt_state: Option<bool>) -> Self {
        let _span = katu_core::trace_fn!("runtime::context::policy_from_config");

        Self {
            selection: selection
                .and_then(SelectionPolicy::parse)
                .unwrap_or(SelectionPolicy::Suffix),
            params: SelectionParams::default(),
            prompt_state: prompt_state.unwrap_or(true),
        }
    }
}

impl Runtime<'_> {
    /// Pré-visualiza a compactação do histórico (E09-T07): determinística, sem I/O.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    pub(crate) fn compaction_preview(&self) -> Result<Option<Compaction>, SessionError> {
        let _span = katu_core::trace_fn!("runtime::context::compaction_preview");

        self.session
            .compact_context(self.budget, CompactionMode::Enabled)
    }

    /// Monta o **contexto efetivo** do turno (E09-T01/T07): prime + (digest) + sufixo cru.
    ///
    /// # Errors
    /// [`SessionError`] se o log estiver corrompido.
    pub(crate) fn context(&self) -> Result<Context, SessionError> {
        let _span = katu_core::trace_fn!("runtime::context::context");

        Ok(self.options()?.context)
    }

    /// Opções de montagem do turno (S-01): orçamento, política de seleção, objetivo e estado.
    fn options(&self) -> Result<Assembly, SessionError> {
        let _span = katu_core::trace_fn!("runtime::context::options");

        self.session.assemble(self.budget, self.assemble_options())
    }

    /// As opções tal como o kernel as consome (a secção de estado é a **já registada**).
    pub(crate) fn assemble_options(&self) -> AssembleOptions<'_> {
        let _span = katu_core::trace_fn!("runtime::context::assemble_options");

        AssembleOptions {
            prime: PrimeMode::Compact,
            compaction: self.compaction,
            selection: self.policy.selection,
            params: self.policy.params,
            goal: &self.goal,
            state: self.state_text.as_deref(),
        }
    }

    /// Política de seleção do contexto (Q-02b/Q-03).
    pub(crate) const fn selection(&self) -> SelectionPolicy {
        self.policy.selection
    }

    /// Fixa a política de seleção (explícito, como a compactação).
    ///
    /// Só existe para os testes e para um interruptor futuro: em produção a política vem da config
    /// do projeto (fechada), lida no arranque.
    #[cfg(test)]
    pub(crate) const fn set_selection(&mut self, selection: SelectionPolicy) {
        self.policy.selection = selection;
    }

    /// A secção `estado` está ligada (Q-04)?
    #[cfg(test)]
    pub(crate) const fn prompt_state(&self) -> bool {
        self.policy.prompt_state
    }

    /// Liga/desliga a secção `estado` no prime (Q-04) — explícito do utilizador.
    ///
    /// Só existe para os testes: em produção a decisão é a chave `behavior.prompt_state`.
    #[cfg(test)]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "toggle explícito (`enabled`), mais legível que um enum de dois valores"
    )]
    pub(crate) const fn set_prompt_state(&mut self, enabled: bool) {
        self.policy.prompt_state = enabled;
    }

    /// Secção `estado` já registada neste turno (para o envelope/`--json`).
    pub(crate) fn state_text(&self) -> Option<&str> {
        let _span = katu_core::trace_fn!("runtime::context::state_text");

        self.state_text.as_deref()
    }

    /// Regista no log a secção `estado` do turno (Q-04) e passa a usá-la no prime.
    ///
    /// É um evento de **controlo** ([`Event::PromptState`]) com o texto exato: sem ele, uma retomada
    /// reconstruiria um prompt de sistema diferente do que foi enviado (E04,
    /// `Model-visible ⟺ logged`). Desligada, não se regista nada e o prime volta a ser estático.
    ///
    /// # Errors
    /// [`SessionError`] se o turno não estiver aberto ou o log recusar o evento.
    pub(crate) fn record_prompt_state(&mut self, max_steps: u32) -> Result<(), SessionError> {
        let _span = katu_core::trace_fn!("runtime::context::record_prompt_state");

        self.max_steps = max_steps;
        if !self.policy.prompt_state {
            self.state_text = None;
            return Ok(());
        }
        let view = StateView {
            mode: if self.plan_mode { "plano" } else { "execucao" },
            rules: &self.enforced,
            steps_max: Some(self.max_steps),
            compaction: self.compaction == CompactionMode::Enabled,
            working_set: &self.session.changed_files()?,
        };
        let text = state_section(&view);
        self.session.apply(&Event::PromptState {
            turn: self.session.state().turn,
            text: text.clone(),
        })?;
        self.state_text = Some(text);
        Ok(())
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
