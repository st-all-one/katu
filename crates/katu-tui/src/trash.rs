//! Vista da lixeira (E10-T07/E06-T09): lista `.katu/trash` e restaura com uma tecla.
//!
//! A lixeira é **recuperável**: restaurar nunca apaga nada e não passa pela política (E06-T09).

use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::layout::centered;

/// Entrada da lixeira mostrada na UI (subset do índice, E06-T09).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    /// Caminho original.
    pub original: String,
    /// Token guardado (usado para restaurar).
    pub stored: String,
}

/// Estado da sobreposição da lixeira: lista + seleção.
#[derive(Debug, Default)]
pub(crate) struct Trash {
    items: Vec<TrashEntry>,
    index: usize,
    open: bool,
}

impl Trash {
    /// Estado inicial (fechada, vazia).
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A sobreposição está aberta.
    #[must_use]
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    /// Entradas listadas (mais recentes primeiro).
    #[must_use]
    pub(crate) fn items(&self) -> &[TrashEntry] {
        &self.items
    }

    /// Índice selecionado.
    #[must_use]
    pub(crate) const fn index(&self) -> usize {
        self.index
    }

    /// Abre a sobreposição.
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    /// Fecha a sobreposição.
    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    /// Substitui a lista (o primeiro fica selecionado).
    pub(crate) fn set_items(&mut self, items: Vec<TrashEntry>) {
        self.items = items;
        self.index = 0;
    }

    /// Move a seleção para cima (satura no topo).
    pub(crate) fn up(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    /// Move a seleção para baixo (satura no fim).
    pub(crate) fn down(&mut self) {
        let next = self.index.saturating_add(1);
        if next < self.items.len() {
            self.index = next;
        }
    }

    /// Token da entrada selecionada, se houver.
    #[must_use]
    pub(crate) fn selected(&self) -> Option<&TrashEntry> {
        self.items.get(self.index)
    }
}

/// Desenha a sobreposição da lixeira.
pub(crate) fn render(frame: &mut Frame<'_>, app: &App) {
    let area = centered(frame.area(), 72, 16);
    frame.render_widget(Clear, area);
    let items = app.trash_items();
    let mut lines: Vec<Line<'static>> = Vec::new();
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "(lixeira vazia)".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    for (index, entry) in items.iter().enumerate() {
        let selected = index == app.trash_index();
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), style),
            Span::styled(entry.original.clone(), style),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "↑/↓ escolhe · r restaura · x esvazia · Esc fecha",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title("lixeira (.katu/trash)"))
            .wrap(Wrap { trim: false }),
        area,
    );
}
