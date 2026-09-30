//! Render **puro** (E10-T03/T06): desenha a partir do [`App`]; nunca faz I/O.
//!
//! O cabeçalho mostra a fase do kernel e a pendência (E10-T06); a conversa e a barra de estado
//! derivam só do `App`.

use katu_core::diag::{Level, events};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::action::Mode;
use crate::app::{App, Role, Status};

/// Desenha um quadro completo a partir do estado.
pub fn render(frame: &mut Frame<'_>, app: &App) {
    let _span = katu_core::span!(Level::Trace, events::TUI_RENDER);
    let [header, body, input, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(header_line(app), header);

    let [conversation, activity] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(34)]).areas(body);

    let lines = transcript_lines(app);
    let offset = bottom_offset(lines.len(), conversation).saturating_sub(app.scroll());
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title("conversa"))
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        conversation,
    );

    let activity_lines = activity_lines(app);
    frame.render_widget(
        Paragraph::new(activity_lines)
            .block(Block::bordered().title("atividade"))
            .wrap(Wrap { trim: false })
            .scroll((bottom_offset(activity_len(app), activity), 0)),
        activity,
    );

    frame.render_widget(
        Paragraph::new(app.input().to_string())
            .block(Block::bordered().title(input_title(app)))
            .wrap(Wrap { trim: false }),
        input,
    );

    frame.render_widget(Paragraph::new(status_line(app)), footer);
}

/// Deslocamento que mostra o **fundo** de um painel (mensagem mais recente).
fn bottom_offset(total: usize, area: Rect) -> u16 {
    let inner = area.height.saturating_sub(2);
    let total = u16::try_from(total).unwrap_or(u16::MAX);
    total.saturating_sub(inner)
}

/// Número de linhas do painel de atividade (sem o histórico de raciocínio).
fn activity_len(app: &App) -> usize {
    app.live()
        .len()
        .saturating_add(app.streaming().lines().count())
}

/// Cabeçalho: identidade, fase e pendência.
fn header_line(app: &App) -> Line<'static> {
    let state = if app.pending() {
        "a pensar…"
    } else {
        "pronto"
    };
    Line::from(vec![
        Span::styled(
            "katu",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  fase {}  ·  {state}", app.phase())),
    ])
}

/// Título do painel de entrada, dependente do modo.
fn input_title(app: &App) -> &'static str {
    if app.mode() == Mode::Insert {
        "mensagem (Enter envia, Esc cancela)"
    } else {
        "mensagem (i ou Enter para escrever)"
    }
}

/// Barra de estado.
fn status_line(app: &App) -> Line<'static> {
    match app.status() {
        Status::Idle => Line::from(Span::styled(
            "q sai  ·  ↑/↓ rola  ·  i escreve".to_string(),
            Style::default().fg(Color::DarkGray),
        )),
        Status::Working => Line::from(Span::styled(
            "a trabalhar…".to_string(),
            Style::default().fg(Color::Yellow),
        )),
        Status::Message(text) => Line::from(Span::raw(text.clone())),
        Status::Failure(text) => Line::from(Span::styled(
            format!("erro: {text}"),
            Style::default().fg(Color::Red),
        )),
    }
}

/// Projeta a conversa em linhas, com prefixo por papel.
fn transcript_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for entry in app.transcript() {
        let style = role_style(entry.role);
        for (index, raw) in entry.text.lines().enumerate() {
            let prefix = if index == 0 {
                role_prefix(entry.role)
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(raw.to_string(), style),
            ]));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(sem mensagens; escreve algo e prime Enter)".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines
}

/// Painel de atividade (E10-T05): tools em curso + texto do modelo a chegar (efémero).
fn activity_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = app
        .live()
        .iter()
        .map(|line| {
            let color = if line.starts_with('⛔') {
                Color::Red
            } else if line.starts_with('⚠') {
                Color::LightYellow
            } else {
                Color::Yellow
            };
            Line::from(Span::styled(line.clone(), Style::default().fg(color)))
        })
        .collect();
    for raw in app.streaming().lines() {
        lines.push(Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(Color::Green),
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(sem atividade)".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines
}

/// Cor de cada papel.
fn role_style(role: Role) -> Style {
    let color = match role {
        Role::User => Color::Cyan,
        Role::Assistant => Color::Green,
        Role::Tool => Color::Yellow,
        Role::Info => Color::DarkGray,
        Role::Error => Color::Red,
    };
    Style::default().fg(color)
}

/// Prefixo curto de cada papel.
const fn role_prefix(role: Role) -> &'static str {
    match role {
        Role::User => "você  ",
        Role::Assistant => "katu  ",
        Role::Tool => "tool  ",
        Role::Info => "info  ",
        Role::Error => "erro  ",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::app::{App, Update};
    use crate::live::Live;

    /// Renderiza num backend de teste e devolve o texto do buffer.
    fn draw(app: &App) -> Result<String, Box<dyn std::error::Error>> {
        let backend = TestBackend::new(60, 16);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| render(frame, app))?;
        let buffer = terminal.backend().buffer();
        Ok(buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect())
    }

    #[test]
    fn renders_header_and_transcript() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.apply_update(Update::Assistant("olá mundo".to_string()));
        let text = draw(&app)?;
        assert!(text.contains("katu"), "{text}");
        assert!(text.contains("olá mundo"), "{text}");
        Ok(())
    }

    #[test]
    fn renders_phase_from_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.apply_update(Update::Phase("verified".to_string()));
        let text = draw(&app)?;
        assert!(text.contains("verified"), "{text}");
        Ok(())
    }

    #[test]
    fn renders_live_activity_panel() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.apply_update(Update::Live(Live::Tool("grep".to_string())));
        app.apply_update(Update::Live(Live::Text("a responder".to_string())));
        let text = draw(&app)?;
        assert!(text.contains("atividade"), "{text}");
        assert!(text.contains("grep"), "{text}");
        assert!(text.contains("a responder"), "{text}");
        Ok(())
    }
}
