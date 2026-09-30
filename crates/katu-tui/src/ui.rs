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
use crate::app::App;
use crate::entry::{Role, Status};
use crate::overlay;
use crate::transcript;
use crate::trash;

/// Teto de entradas da conversa projetadas por quadro (E10-T03).
const MAX_TRANSCRIPT_ENTRIES: usize = 200;
/// Teto de linhas do painel de atividade por quadro (E10-T03).
const MAX_ACTIVITY_LINES: usize = 100;

/// Desenha um quadro completo a partir do estado.
pub fn render(frame: &mut Frame<'_>, app: &App) {
    let _span = katu_core::fn_span!(Level::Trace, events::TUI_RENDER, "ui::render");
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

    let (input_text, input_title) = if app.steering().is_empty() {
        (app.input().to_string(), input_title(app))
    } else {
        (
            app.steering().to_string(),
            "steering (Enter enfileira, Esc cancela)",
        )
    };
    frame.render_widget(
        Paragraph::new(input_text)
            .block(Block::bordered().title(input_title))
            .wrap(Wrap { trim: false }),
        input,
    );

    frame.render_widget(Paragraph::new(status_line(app)), footer);

    if app.trash_open() {
        trash::render(frame, app);
    }
    if app.viewer_open() {
        transcript::render(frame, app);
    }
    if app.help_open() {
        overlay::help(frame);
    }
    if app.menu().is_some() {
        overlay::menu(frame, app);
    }
}

/// Deslocamento que mostra o **fundo** de um painel (mensagem mais recente).
fn bottom_offset(total: usize, area: Rect) -> u16 {
    let _span = katu_core::trace_fn!("ui::bottom_offset");

    let inner = area.height.saturating_sub(2);
    let total = u16::try_from(total).unwrap_or(u16::MAX);
    total.saturating_sub(inner)
}

/// Número de linhas do painel de atividade (sem o histórico de raciocínio).
fn activity_len(app: &App) -> usize {
    let _span = katu_core::trace_fn!("ui::activity_len");

    let live = usize::min(app.live().len(), MAX_ACTIVITY_LINES);
    live.saturating_add(app.streaming().lines().count())
}

/// Cabeçalho: identidade, modelo, pensamento, fase, pendência e próxima ação.
fn header_line(app: &App) -> Line<'static> {
    let _span = katu_core::trace_fn!("ui::header_line");

    let state = if app.pending() {
        "a pensar…"
    } else {
        "pronto"
    };
    let model = app.model().unwrap_or("—");
    let reasoning = format!("{:?}", app.reasoning()).to_lowercase();
    let mut spans = vec![
        Span::styled(
            "katu",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {model} · pensamento {reasoning} · fase {}  ·  {state}",
            app.phase()
        )),
    ];
    if let Some(next) = app.next_action() {
        spans.push(Span::raw(format!("  ·  próximo {next}")));
    }
    if let Some(usage) = app.usage() {
        spans.push(Span::raw(format!("  ·  {usage}")));
    }
    Line::from(spans)
}

/// Título do painel de entrada, dependente do modo.
fn input_title(app: &App) -> &'static str {
    let _span = katu_core::trace_fn!("ui::input_title");

    if app.mode() == Mode::Insert {
        "mensagem (Enter envia, Esc cancela)"
    } else {
        "mensagem (i ou Enter para escrever)"
    }
}

/// Barra de estado.
fn status_line(app: &App) -> Line<'static> {
    let _span = katu_core::trace_fn!("ui::status_line");

    match app.status() {
        Status::Idle if app.plan_mode() => Line::from(Span::styled(
            "PLANO (escrita só sob .katu/)  ·  q sai  ·  /plan desliga  ·  ? ajuda".to_string(),
            Style::default().fg(Color::Cyan),
        )),
        Status::Idle => Line::from(Span::styled(
            "q sai  ·  ↑/↓ rola  ·  i escreve  ·  / comandos  ·  ? ajuda".to_string(),
            Style::default().fg(Color::DarkGray),
        )),
        Status::Working => Line::from(Span::styled(
            "a trabalhar…  ·  Esc cancela".to_string(),
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
    let _span = katu_core::trace_fn!("ui::transcript_lines");

    let mut lines: Vec<Line<'static>> = Vec::new();
    let start = app
        .transcript()
        .len()
        .saturating_sub(MAX_TRANSCRIPT_ENTRIES);
    for entry in app.transcript().iter().skip(start) {
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
    let _span = katu_core::trace_fn!("ui::activity_lines");

    let start = app.live().len().saturating_sub(MAX_ACTIVITY_LINES);
    let mut lines: Vec<Line<'static>> = app
        .live()
        .iter()
        .skip(start)
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
    let _span = katu_core::trace_fn!("ui::role_style");

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
