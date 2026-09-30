//! Tipos de configuração e débito do cost governor (E09-T06).

use std::collections::BTreeMap;

use katu_policy::ToolName;

use crate::kernel::budget::{BudgetCap, Charge};

/// Janela rolante de chamadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingWindowCap {
    /// Máximo de chamadas dentro da janela.
    pub max_calls: u32,
    /// Largura da janela (ms).
    pub window_ms: u64,
}

/// Velocidade financeira (custo por minuto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VelocityCap {
    /// Teto de micro-unidades por minuto.
    pub max_micros_per_minute: u64,
}

/// Camadas de teto do governor.
#[derive(Debug, Clone, Default)]
pub struct CostCaps {
    /// Teto global/tarefa (`max_tokens`, `max_turns`, chamadas, tempo).
    pub global: BudgetCap,
    /// Teto de chamadas por ferramenta.
    pub per_tool: BTreeMap<ToolName, u32>,
    /// Janela rolante (opcional).
    pub rolling: Option<RollingWindowCap>,
    /// Velocidade financeira (opcional).
    pub velocity: Option<VelocityCap>,
}

/// Débito submetido ao governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostCharge {
    /// Ferramenta associada (só em `ToolCall`).
    pub tool: Option<ToolName>,
    /// Débito clássico de orçamento.
    pub charge: Charge,
    /// Custo financeiro (micro-unidades).
    pub micros: u64,
    /// Instante do débito (ms); `None` desativa as camadas temporais.
    pub now_millis: Option<u64>,
}

impl CostCharge {
    /// Débito de abrir um turno.
    #[must_use]
    pub const fn turn() -> Self {
        Self {
            tool: None,
            charge: Charge::Turn,
            micros: 0,
            now_millis: None,
        }
    }

    /// Débito de uma chamada de ferramenta.
    #[must_use]
    pub const fn tool_call(tool: ToolName) -> Self {
        Self {
            tool: Some(tool),
            charge: Charge::ToolCall,
            micros: 0,
            now_millis: None,
        }
    }

    /// Acrescenta custo financeiro.
    #[must_use]
    pub const fn with_micros(mut self, micros: u64) -> Self {
        self.micros = micros;
        self
    }

    /// Fixa o instante (ativa as camadas temporais).
    #[must_use]
    pub const fn at(mut self, now_millis: u64) -> Self {
        self.now_millis = Some(now_millis);
        self
    }
}

/// Kill switch engatado (com o instante do corte).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillSwitch {
    /// Motivo.
    pub reason: String,
    /// Instante do corte (ms).
    pub tripped_at_millis: u64,
}

/// Autorização de re-enable do kill switch (assinada por humano).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reenable {
    reason: String,
    authorized_by: String,
}

impl Reenable {
    /// Constrói a autorização; motivo e autor são obrigatórios (o agente não assina).
    ///
    /// # Errors
    /// [`ReenableError`] se o motivo ou o autor forem vazios.
    pub fn new(
        reason: impl Into<String>,
        authorized_by: impl Into<String>,
    ) -> Result<Self, ReenableError> {
        let _span = crate::trace_fn!("kernel::cost::caps::new");

        let reason = reason.into();
        let authorized_by = authorized_by.into();
        if reason.trim().is_empty() {
            return Err(ReenableError::EmptyReason);
        }
        if authorized_by.trim().is_empty() {
            return Err(ReenableError::EmptyAuthorizedBy);
        }
        Ok(Self {
            reason,
            authorized_by,
        })
    }

    /// Motivo.
    #[must_use]
    pub fn reason(&self) -> &str {
        let _span = crate::trace_fn!("kernel::cost::caps::reason");

        &self.reason
    }

    /// Autor.
    #[must_use]
    pub fn authorized_by(&self) -> &str {
        let _span = crate::trace_fn!("kernel::cost::caps::authorized_by");

        &self.authorized_by
    }
}

/// Erro de autorização de re-enable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReenableError {
    /// Sem motivo.
    #[error("re-enable sem motivo")]
    EmptyReason,
    /// Sem autor.
    #[error("re-enable sem autor")]
    EmptyAuthorizedBy,
}
