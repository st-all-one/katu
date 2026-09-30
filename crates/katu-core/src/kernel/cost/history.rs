//! Histórico temporal do cost governor: janela rolante, velocidade e retomada (E09-T06/ADR 0008).
//!
//! As camadas temporais dependem do relógio, que o log **não** reproduz. O snapshot guarda o
//! histórico `(ms, micros)` e a retomada restaura-o exatamente, sem varrer o prefixo do log.

use std::collections::BTreeMap;

use katu_policy::ToolName;

use super::CostGovernor;
use super::caps::CostCaps;
use crate::kernel::budget::Budget;

impl CostGovernor {
    /// Reconstrói o governor com o histórico temporal já observado (retomada de snapshot).
    #[must_use]
    pub fn with_history(
        caps: CostCaps,
        usage: Budget,
        per_tool_used: BTreeMap<ToolName, u32>,
        history: impl IntoIterator<Item = (u64, u64)>,
    ) -> Self {
        let mut governor = Self::with_usage(caps, usage, per_tool_used);
        governor.history = history.into_iter().collect();
        governor
    }

    /// Histórico temporal `(now_millis, micros)` ainda retido.
    pub fn history(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.history.iter().copied()
    }

    /// Chamadas registadas estritamente dentro da janela.
    pub(super) fn calls_in_window(&self, now: u64, window_ms: u64) -> u32 {
        let count = self
            .history
            .iter()
            .filter(|entry| now.saturating_sub(entry.0) < window_ms)
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// Custo registado estritamente dentro da janela.
    pub(super) fn micros_in_window(&self, now: u64, window_ms: u64) -> u64 {
        self.history
            .iter()
            .filter(|entry| now.saturating_sub(entry.0) < window_ms)
            .fold(0_u64, |acc, entry| acc.saturating_add(entry.1))
    }

    /// Descarta histórico fora da maior janela configurada.
    pub(super) fn prune(&mut self, now: u64) {
        let retention = self.retention_ms();
        if retention == 0 {
            return;
        }
        while self
            .history
            .front()
            .is_some_and(|entry| now.saturating_sub(entry.0) >= retention)
        {
            self.history.pop_front();
        }
    }

    /// Maior janela configurada (0 = sem camadas temporais).
    fn retention_ms(&self) -> u64 {
        let rolling = self.caps.rolling.map_or(0, |rolling| rolling.window_ms);
        let velocity = if self.caps.velocity.is_some() {
            super::VELOCITY_WINDOW_MS
        } else {
            0
        };
        rolling.max(velocity)
    }
}
