//! Cost governor (E09-T06): camadas de teto avaliadas do mais **específico** para o mais
//! **global**, com um kill switch que só reabre com autorização **separada**.
//!
//! Ordem de avaliação (o primeiro que dispara vence):
//! `KillSwitch → PerTool → RollingWindow → FinancialVelocity → Global`.
//!
//! O teto **por ferramenta** é avaliado antes do teto **global** (aceite de E09-T06): um loop
//! patológico é cortado na camada específica, com a causa exata. Nenhuma recusa altera o uso
//! (§29) — o chamador verifica com [`CostGovernor::check`] e só depois aplica com
//! [`CostGovernor::commit`].
//!
//! As camadas temporais (janela rolante, velocidade financeira) só são avaliadas quando o débito
//! traz `now_millis` (o log não é reprodutível nesse eixo).

mod caps;
mod history;
mod refusal;

use std::collections::{BTreeMap, VecDeque};

use katu_policy::ToolName;

use crate::diag::{Level, events};
use crate::kernel::budget::{Budget, BudgetGate};
use crate::kernel::event::Event;

pub use caps::{
    CostCaps, CostCharge, KillSwitch, Reenable, ReenableError, RollingWindowCap, VelocityCap,
};
pub use refusal::{CostLayer, CostRefusal};

/// Milissegundos de uma janela de velocidade financeira.
const VELOCITY_WINDOW_MS: u64 = 60_000;

/// Governor com as camadas de teto e o uso corrente.
#[derive(Debug, Clone)]
pub struct CostGovernor {
    caps: CostCaps,
    global: BudgetGate,
    per_tool_used: BTreeMap<ToolName, u32>,
    history: VecDeque<(u64, u64)>,
    kill: Option<KillSwitch>,
}

impl CostGovernor {
    /// Cria o governor com os tetos dados e uso nulo.
    #[must_use]
    pub fn new(caps: CostCaps, usage: Budget) -> Self {
        Self::with_usage(caps, usage, BTreeMap::new())
    }

    /// Cria o governor com uso de ferramenta já observado (retoma de um log).
    #[must_use]
    pub fn with_usage(
        caps: CostCaps,
        usage: Budget,
        per_tool_used: BTreeMap<ToolName, u32>,
    ) -> Self {
        Self {
            global: BudgetGate::resume(caps.global, usage),
            caps,
            per_tool_used,
            history: VecDeque::new(),
            kill: None,
        }
    }

    /// Reconstrói o governor a partir do log (uso global e por ferramenta).
    #[must_use]
    pub fn from_events(caps: CostCaps, events: &[Event]) -> Self {
        let mut per_tool: BTreeMap<ToolName, u32> = BTreeMap::new();
        for event in events {
            if let Event::ToolCall { tool, .. } = event {
                let entry = per_tool.entry(tool.name).or_insert(0_u32);
                *entry = entry.saturating_add(1);
            }
        }
        Self::with_usage(caps, Budget::from_events(events), per_tool)
    }

    /// Tetos correntes.
    #[must_use]
    pub fn caps(&self) -> &CostCaps {
        &self.caps
    }

    /// Portão global/tarefa (único dono do teto de contexto).
    #[must_use]
    pub fn global(&self) -> BudgetGate {
        self.global
    }

    /// Kill switch, se engatado.
    #[must_use]
    pub fn kill_switch(&self) -> Option<&KillSwitch> {
        self.kill.as_ref()
    }

    /// Uso por ferramenta.
    #[must_use]
    pub fn per_tool_used(&self) -> &BTreeMap<ToolName, u32> {
        &self.per_tool_used
    }

    /// Verifica um débito **sem** o aplicar (nenhuma camada muda em recusa, §29).
    ///
    /// # Errors
    /// [`CostRefusal`] na primeira camada que dispara.
    #[cfg_attr(
        not(feature = "instrument"),
        allow(
            unused_variables,
            reason = "o macro no-op ignora os campos (custo zero)"
        )
    )]
    pub fn check(&self, charge: &CostCharge) -> Result<(), CostRefusal> {
        let _span = crate::span!(Level::Trace, events::COST_CHECK, "tool" => charge.tool.is_some());
        let result = self.check_layers(charge);
        if let Err(refusal) = &result {
            crate::event!(
                Level::Warn,
                events::COST_REFUSE,
                "layer" => refusal.layer().as_str(),
            );
        }
        result
    }

    /// Aplica um débito (o chamador deve ter verificado com [`CostGovernor::check`]).
    pub fn commit(&mut self, charge: &CostCharge) {
        if let Some(tool) = charge.tool {
            let entry = self.per_tool_used.entry(tool).or_insert(0_u32);
            *entry = entry.saturating_add(1);
        }
        self.global.commit(charge.charge);
        if let Some(now) = charge.now_millis {
            self.prune(now);
            self.history.push_back((now, charge.micros));
        }
    }

    /// Engata o kill switch (corta tudo até re-enable separado).
    pub fn trip(&mut self, reason: impl Into<String>, now_millis: u64) {
        let reason = reason.into();
        crate::event!(Level::Warn, events::COST_KILL, "reason" => reason.as_str());
        self.kill = Some(KillSwitch {
            reason,
            tripped_at_millis: now_millis,
        });
    }

    /// Reabre após o kill switch (exige autorização assinada por humano).
    #[cfg_attr(
        not(feature = "instrument"),
        allow(
            unused_variables,
            reason = "o macro no-op ignora os campos (custo zero)"
        )
    )]
    pub fn reenable(&mut self, grant: &Reenable) {
        crate::event!(
            Level::Info,
            events::COST_REENABLE,
            "by" => grant.authorized_by(),
        );
        self.kill = None;
    }

    /// Avalia as camadas pela ordem de precedência.
    fn check_layers(&self, charge: &CostCharge) -> Result<(), CostRefusal> {
        self.check_kill()?;
        if let Some(tool) = charge.tool {
            self.check_per_tool(tool)?;
        }
        if let Some(now) = charge.now_millis {
            if charge.tool.is_some() {
                self.check_rolling(now)?;
            }
            self.check_velocity(now, charge.micros)?;
        }
        self.global.check(charge.charge)?;
        Ok(())
    }

    /// Kill switch.
    fn check_kill(&self) -> Result<(), CostRefusal> {
        match &self.kill {
            Some(kill) => Err(CostRefusal::KillSwitch {
                reason: kill.reason.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Teto por ferramenta.
    fn check_per_tool(&self, tool: ToolName) -> Result<(), CostRefusal> {
        let Some(cap) = self.caps.per_tool.get(&tool).copied() else {
            return Ok(());
        };
        let used = self
            .per_tool_used
            .get(&tool)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        if used > cap {
            return Err(CostRefusal::PerTool { tool, used, cap });
        }
        Ok(())
    }

    /// Janela rolante.
    fn check_rolling(&self, now: u64) -> Result<(), CostRefusal> {
        let Some(rolling) = self.caps.rolling else {
            return Ok(());
        };
        let calls = self
            .calls_in_window(now, rolling.window_ms)
            .saturating_add(1);
        if calls > rolling.max_calls {
            return Err(CostRefusal::RollingWindow {
                calls,
                window_ms: rolling.window_ms,
            });
        }
        Ok(())
    }

    /// Velocidade financeira.
    fn check_velocity(&self, now: u64, micros: u64) -> Result<(), CostRefusal> {
        let Some(velocity) = self.caps.velocity else {
            return Ok(());
        };
        let spent = self
            .micros_in_window(now, VELOCITY_WINDOW_MS)
            .saturating_add(micros);
        if spent > velocity.max_micros_per_minute {
            return Err(CostRefusal::FinancialVelocity {
                micros: spent,
                cap: velocity.max_micros_per_minute,
            });
        }
        Ok(())
    }
}
/// Débito implícito de um evento (turnos e chamadas; as temporais vêm do clock).
#[must_use]
pub fn cost_charge_for(event: &Event) -> Option<CostCharge> {
    match event {
        Event::TurnStart { .. } => Some(CostCharge::turn()),
        Event::ToolCall { tool, .. } => Some(CostCharge::tool_call(tool.name)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
