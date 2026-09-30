//! Orçamento e portão de teto (E04-T07): recusa determinística ao atingir um teto.
//!
//! O [`BudgetGate`] é o **único dono do teto de contexto** (§51.8): a política pode ter regras de
//! orçamento, mas não mantém um segundo teto. Um teto atingido é uma **recusa** — nunca se "corta a
//! evidência" para caber no orçamento (§29).

use serde::{Deserialize, Serialize};

use super::event::Event;
use crate::diag::{Level, events};

/// Consumo acumulado de uma tarefa.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Turnos abertos.
    pub turns: u32,
    /// Pedidos de tool.
    pub tool_calls: u32,
    /// Tokens consumidos (reportados pelo provider).
    pub tokens: u64,
    /// Tempo de parede acumulado, em milissegundos.
    pub wall_clock_ms: u64,
}

impl Budget {
    /// Orçamento nulo.
    pub const ZERO: Self = Self {
        turns: 0,
        tool_calls: 0,
        tokens: 0,
        wall_clock_ms: 0,
    };

    /// Uso após aplicar um débito (saturação, sem `panic`).
    #[must_use]
    pub const fn after(self, charge: Charge) -> Self {
        match charge {
            Charge::Turn => Self {
                turns: self.turns.saturating_add(1),
                ..self
            },
            Charge::ToolCall => Self {
                tool_calls: self.tool_calls.saturating_add(1),
                ..self
            },
            Charge::Tokens(tokens) => Self {
                tokens: self.tokens.saturating_add(tokens),
                ..self
            },
            Charge::WallClock(millis) => Self {
                wall_clock_ms: self.wall_clock_ms.saturating_add(millis),
                ..self
            },
        }
    }

    /// Uso reconstruído a partir de uma sequência de eventos (turnos e chamadas).
    ///
    /// Tokens e tempo de parede não são reprodutíveis a partir do log e ficam a zero.
    #[must_use]
    pub fn from_events(events: &[Event]) -> Self {
        let _span = crate::trace_fn!("kernel::budget::from_events");

        let mut usage = Self::ZERO;
        for event in events {
            if let Some(charge) = charge_for(event) {
                let _span = crate::span!(Level::Trace, events::KERNEL_BUDGET);
                usage = usage.after(charge);
            }
        }
        usage
    }
}

/// Débito de orçamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Charge {
    /// Abrir um turno.
    Turn,
    /// Registar um pedido de tool.
    ToolCall,
    /// Consumir tokens.
    Tokens(u64),
    /// Consumir tempo de parede (milissegundos).
    WallClock(u64),
}

/// Tetos opcionais; `None` significa "sem teto" nesse eixo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetCap {
    /// Teto de turnos.
    pub turns: Option<u32>,
    /// Teto de chamadas de tool.
    pub tool_calls: Option<u32>,
    /// Teto de tokens.
    pub tokens: Option<u64>,
    /// Teto de tempo de parede (milissegundos).
    pub wall_clock_ms: Option<u64>,
}

impl BudgetCap {
    /// Sem qualquer teto.
    pub const NONE: Self = Self {
        turns: None,
        tool_calls: None,
        tokens: None,
        wall_clock_ms: None,
    };

    /// Primeira violação, se o uso projetado exceder algum teto.
    ///
    /// O teto é atingido quando o uso **ultrapassa** o cap (`used > cap`): `used == cap` ainda é
    /// aceite (o teto foi alcançado, não excedido) e o débito seguinte é que é recusado.
    #[must_use]
    pub const fn breach(self, usage: Budget) -> Option<BudgetRefusal> {
        if let Some(cap) = self.turns
            && usage.turns > cap
        {
            return Some(BudgetRefusal::Turns {
                used: usage.turns,
                cap,
            });
        }
        if let Some(cap) = self.tool_calls
            && usage.tool_calls > cap
        {
            return Some(BudgetRefusal::ToolCalls {
                used: usage.tool_calls,
                cap,
            });
        }
        if let Some(cap) = self.tokens
            && usage.tokens > cap
        {
            return Some(BudgetRefusal::Tokens {
                used: usage.tokens,
                cap,
            });
        }
        if let Some(cap) = self.wall_clock_ms
            && usage.wall_clock_ms > cap
        {
            return Some(BudgetRefusal::WallClock {
                used: usage.wall_clock_ms,
                cap,
            });
        }
        None
    }
}

/// Recusa determinística por teto de orçamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case", tag = "axis")]
#[non_exhaustive]
pub enum BudgetRefusal {
    /// Teto de turnos excedido.
    #[error("teto de turnos: {used}/{cap}")]
    Turns {
        /// Uso projetado.
        used: u32,
        /// Teto.
        cap: u32,
    },
    /// Teto de chamadas excedido.
    #[error("teto de chamadas: {used}/{cap}")]
    ToolCalls {
        /// Uso projetado.
        used: u32,
        /// Teto.
        cap: u32,
    },
    /// Teto de tokens excedido.
    #[error("teto de tokens: {used}/{cap}")]
    Tokens {
        /// Uso projetado.
        used: u64,
        /// Teto.
        cap: u64,
    },
    /// Teto de tempo de parede excedido.
    #[error("teto de tempo: {used}ms/{cap}ms")]
    WallClock {
        /// Uso projetado.
        used: u64,
        /// Teto.
        cap: u64,
    },
}

impl BudgetRefusal {
    /// Nome estável do eixo do teto (para telemetria).
    #[must_use]
    pub const fn axis(self) -> &'static str {
        match self {
            Self::Turns { .. } => "turns",
            Self::ToolCalls { .. } => "tool_calls",
            Self::Tokens { .. } => "tokens",
            Self::WallClock { .. } => "wall_clock",
        }
    }

    /// Uso projetado que disparou a recusa.
    #[must_use]
    pub fn used(self) -> u64 {
        let _span = crate::trace_fn!("kernel::budget::used");

        match self {
            Self::Turns { used, .. } | Self::ToolCalls { used, .. } => u64::from(used),
            Self::Tokens { used, .. } | Self::WallClock { used, .. } => used,
        }
    }

    /// Teto violado.
    #[must_use]
    pub fn cap(self) -> u64 {
        let _span = crate::trace_fn!("kernel::budget::cap");

        match self {
            Self::Turns { cap, .. } | Self::ToolCalls { cap, .. } => u64::from(cap),
            Self::Tokens { cap, .. } | Self::WallClock { cap, .. } => cap,
        }
    }
}

/// Portão de orçamento com tetos e uso correntes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetGate {
    cap: BudgetCap,
    usage: Budget,
}

impl BudgetGate {
    /// Cria um portão com os tetos dados e uso nulo.
    #[must_use]
    pub const fn new(cap: BudgetCap) -> Self {
        Self {
            cap,
            usage: Budget::ZERO,
        }
    }

    /// Portão sem tetos.
    #[must_use]
    pub const fn unlimited() -> Self {
        Self::new(BudgetCap::NONE)
    }

    /// Retoma um portão com os tetos e o uso já observado (reconstrução a partir do log).
    #[must_use]
    pub const fn resume(cap: BudgetCap, usage: Budget) -> Self {
        Self { cap, usage }
    }

    /// Uso corrente.
    #[must_use]
    pub const fn usage(&self) -> Budget {
        self.usage
    }

    /// Tetos correntes.
    #[must_use]
    pub const fn cap(&self) -> BudgetCap {
        self.cap
    }

    /// Verifica um débito **sem** o aplicar. O uso fica inalterado mesmo em recusa (§29).
    ///
    /// # Errors
    /// [`BudgetRefusal`] se o uso projetado exceder um teto.
    pub fn check(&self, charge: Charge) -> Result<(), BudgetRefusal> {
        let _span = crate::trace_fn!("kernel::budget::check");

        let projected = self.usage.after(charge);
        if let Some(refusal) = self.cap.breach(projected) {
            crate::event!(
                Level::Warn,
                events::KERNEL_BUDGET_REFUSE,
                "axis" => refusal.axis(),
                "used" => refusal.used(),
                "cap" => refusal.cap(),
            );
            return Err(refusal);
        }
        Ok(())
    }

    /// Aplica um débito. O chamador deve ter verificado com [`BudgetGate::check`].
    pub fn commit(&mut self, charge: Charge) {
        let _span = crate::trace_fn!("kernel::budget::commit");

        self.usage = self.usage.after(charge);
    }
}

/// Débito implícito de um evento (turnos e chamadas contam; o resto é explícito).
#[must_use]
pub const fn charge_for(event: &Event) -> Option<Charge> {
    match event {
        Event::TurnStart { .. } => Some(Charge::Turn),
        Event::ToolCall { .. } => Some(Charge::ToolCall),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
