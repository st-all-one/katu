//! Render do challenge (E10-T04): sobreposição centrada, sem I/O.

use katu_core::diag::{Level, events};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::{Challenge, Focus, QUESTIONS};

/// Desenha o challenge centrado na área.
pub(crate) fn render(frame: &mut Frame<'_>, challenge: &Challenge) {
    let _span = katu_core::span!(Level::Trace, events::TUI_RENDER);
    let area = centered(frame.area(), 64, 12);
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("tool ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                challenge.prompt().tool.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("regra ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                challenge.prompt().rule.clone(),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled("âmbito ", Style::default().fg(Color::DarkGray)),
            Span::styled(challenge.prompt().scope.clone(), Style::default()),
        ]),
        Line::default(),
    ];
    for (index, question) in QUESTIONS.iter().enumerate() {
        let mark = if challenge.checked().get(index).copied().unwrap_or(false) {
            "[x]"
        } else {
            "[ ]"
        };
        let pointer = if challenge.focus() == Focus::Checklist && challenge.cursor() == index {
            ">"
        } else {
            " "
        };
        lines.push(Line::from(format!("{pointer} {mark} {question}")));
    }
    lines.push(Line::default());
    let reason_style = if challenge.focus() == Focus::Reason {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::styled("motivo> ", Style::default().fg(Color::DarkGray)),
        Span::styled(challenge.reason().to_string(), reason_style),
    ]));
    lines.push(Line::from(Span::styled(
        "Tab foco · espaço marca · Enter aprova · Esc cancela",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title("aprovação (challenge-and-response)"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Área centrada de tamanho `width`×`height` (limitada à moldura).
fn centered(area: Rect, width: u16, height: u16) -> Rect {
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
