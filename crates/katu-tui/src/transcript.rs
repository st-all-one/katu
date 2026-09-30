//! Vista da transcrição durável (E10-T05): **read-only**, com scroll; nunca edita.
//!
//! A transcrição é a projeção do **log** (durável); a borda lê o ficheiro `.katu/transcript.md` e
//! injeta-a via `Update::Transcript`. Esta vista só a apresenta — o painel de atividade continua
//! efémero e separado (§50.3).

use katu_core::diag::{Level, events};
use ratatui::Frame;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::App;

/// Estado da vista da transcrição: linhas + deslocamento (índice da primeira linha visível).
#[derive(Debug, Default)]
pub(crate) struct TranscriptView {
    lines: Vec<String>,
    scroll: u16,
    open: bool,
}

impl TranscriptView {
    /// Estado inicial (fechada, vazia).
    #[must_use]
    pub(crate) fn new() -> Self {
        let _span = katu_core::trace_fn!("transcript::new");

        Self::default()
    }

    /// A vista está aberta.
    #[must_use]
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    /// Linhas da transcrição.
    #[must_use]
    pub(crate) fn lines(&self) -> &[String] {
        let _span = katu_core::trace_fn!("transcript::lines");

        &self.lines
    }

    /// Deslocamento (índice da primeira linha visível).
    #[must_use]
    pub(crate) const fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Abre a vista.
    pub(crate) fn open(&mut self) {
        let _span = katu_core::trace_fn!("transcript::open");

        self.open = true;
    }

    /// Fecha a vista.
    pub(crate) fn close(&mut self) {
        let _span = katu_core::trace_fn!("transcript::close");

        self.open = false;
    }

    /// Substitui as linhas (recomeça no topo).
    pub(crate) fn set_lines(&mut self, lines: Vec<String>) {
        let _span = katu_core::trace_fn!("transcript::set_lines");

        self.lines = lines;
        self.scroll = 0;
    }

    /// Rola para cima (satura no topo).
    pub(crate) fn up(&mut self) {
        let _span = katu_core::trace_fn!("transcript::up");

        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Rola para baixo (satura na última linha).
    pub(crate) fn down(&mut self) {
        let _span = katu_core::trace_fn!("transcript::down");

        let last = u16::try_from(self.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
        self.scroll = self.scroll.saturating_add(1).min(last);
    }
}

/// Desenha a vista da transcrição (ecrã inteiro, read-only).
pub(crate) fn render(frame: &mut Frame<'_>, app: &App) {
    let _span = katu_core::fn_span!(Level::Trace, events::TUI_RENDER, "transcript::render");
    let area = frame.area();
    frame.render_widget(Clear, area);
    let lines: Vec<Line<'static>> = app
        .viewer_lines()
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::bordered().title("transcrição durável (read-only) · ↑/↓ rola · Esc fecha"),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.viewer_scroll(), 0)),
        area,
    );
}
