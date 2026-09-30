//! Utilitários de layout das sobreposições (E10): painel centrado.
//!
//! Partilhado pelo challenge de aprovação (E10-T04) e pela vista da lixeira (E10-T07).

use ratatui::layout::Rect;

/// Área centrada de tamanho `width`×`height` (limitada à moldura).
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let _span = katu_core::trace_fn!("layout::centered");

    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}
