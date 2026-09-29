//! Camada e recusa do cost governor (E09-T06).

use katu_policy::ToolName;

use crate::kernel::budget::BudgetRefusal;

/// Camada de teto (para telemetria e testes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CostLayer {
    /// Kill switch engatado.
    KillSwitch,
    /// Teto por ferramenta.
    PerTool,
    /// Janela rolante de chamadas.
    RollingWindow,
    /// Velocidade financeira (custo por minuto).
    FinancialVelocity,
    /// Teto global/tarefa.
    Global,
}

impl CostLayer {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KillSwitch => "kill_switch",
            Self::PerTool => "per_tool",
            Self::RollingWindow => "rolling_window",
            Self::FinancialVelocity => "financial_velocity",
            Self::Global => "global",
        }
    }
}

/// Recusa determinística do cost governor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CostRefusal {
    /// Kill switch engatado.
    #[error("kill switch ativo: {reason}")]
    KillSwitch {
        /// Motivo do corte.
        reason: String,
    },
    /// Teto por ferramenta excedido.
    #[error("teto por ferramenta {tool:?}: {used}/{cap}")]
    PerTool {
        /// Ferramenta.
        tool: ToolName,
        /// Uso projetado.
        used: u32,
        /// Teto.
        cap: u32,
    },
    /// Teto global/tarefa excedido.
    #[error("teto global: {0}")]
    Global(#[from] BudgetRefusal),
    /// Janela rolante excedida.
    #[error("janela rolante: {calls} chamadas/{window_ms}ms")]
    RollingWindow {
        /// Chamadas projetadas na janela.
        calls: u32,
        /// Largura da janela (ms).
        window_ms: u64,
    },
    /// Velocidade financeira excedida.
    #[error("velocidade financeira: {micros}µ/min (teto {cap}µ)")]
    FinancialVelocity {
        /// Custo projetado na janela.
        micros: u64,
        /// Teto por minuto.
        cap: u64,
    },
}

impl CostRefusal {
    /// Camada que disparou.
    #[must_use]
    pub const fn layer(&self) -> CostLayer {
        match self {
            Self::KillSwitch { .. } => CostLayer::KillSwitch,
            Self::PerTool { .. } => CostLayer::PerTool,
            Self::Global(_) => CostLayer::Global,
            Self::RollingWindow { .. } => CostLayer::RollingWindow,
            Self::FinancialVelocity { .. } => CostLayer::FinancialVelocity,
        }
    }
}
