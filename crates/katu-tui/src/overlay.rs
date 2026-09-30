//! Sobreposições de ajuda e mini-menu (E20-T10).
//!
//! Render **puro**: a ajuda (`?`) lista os comandos `/`, os padrões e as teclas; o mini-menu
//! mostra as escolhas de `/model`/`/thinking` com a seleção destacada.

use katu_core::diag::{Level, events};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::App;

/// Comandos `/` disponíveis.
const COMMANDS: &[(&str, &str)] = &[
    ("/model", "escolhe o modelo (menu)"),
    ("/thinking", "escolhe o grau de pensamento (menu)"),
    ("/compact", "liga/desliga a compactação do histórico"),
    (
        "/plan",
        "liga/desliga o modo de planeamento (escrita só sob .katu/)",
    ),
    ("/verify", "corre o gate de verificação"),
    ("/trash", "abre a lixeira"),
    ("/transcript", "abre a transcrição durável"),
    ("/help", "esta ajuda"),
    ("/quit", "sai da UI"),
];

/// Padrões da linha de mensagem.
const PATTERNS: &[(&str, &str)] = &[
    ("/<comando>", "comando da UI"),
    ("!<cmd>", "shell pela política (bloqueado em /plan)"),
    ("@<path>", "cita um caminho (só o caminho)"),
    ("/skill:<nome>", "força o carregamento de uma skill"),
];

/// Teclas.
const KEYS: &[(&str, &str)] = &[
    ("Enter/i", "escrever"),
    ("/", "começar um comando"),
    ("?", "ajuda"),
    ("↑/↓", "rolar"),
    ("Esc", "cancela a rodada / fecha a sobreposição"),
    ("q", "sair"),
];

/// Largura/altura da sobreposição de ajuda.
const HELP_SIZE: (u16, u16) = (64, 26);
/// Largura/altura do mini-menu.
const MENU_SIZE: (u16, u16) = (48, 14);

/// Desenha a sobreposição de ajuda (`?`).
pub(crate) fn help(frame: &mut Frame<'_>) {
    let _span = katu_core::fn_span!(Level::Trace, events::TUI_RENDER, "overlay::help");
    let area = centered(frame.area(), HELP_SIZE);
    frame.render_widget(Clear, area);
    let mut lines = vec![section("comandos")];
    lines.extend(COMMANDS.iter().map(|(name, help)| entry(name, help)));
    lines.push(section("padrões"));
    lines.extend(PATTERNS.iter().map(|(name, help)| entry(name, help)));
    lines.push(section("teclas"));
    lines.extend(KEYS.iter().map(|(name, help)| entry(name, help)));
    lines.push(Line::from(Span::styled(
        "Esc/q/? fecham",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title("ajuda — comandos e teclas"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Desenha o mini-menu corrente (E20-T10).
pub(crate) fn menu(frame: &mut Frame<'_>, app: &App) {
    let _span = katu_core::fn_span!(Level::Trace, events::TUI_RENDER, "overlay::menu");
    let Some(menu) = app.menu() else {
        return;
    };
    let area = centered(frame.area(), MENU_SIZE);
    frame.render_widget(Clear, area);
    let lines: Vec<Line<'static>> = menu
        .items()
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let selected = index == menu.index();
            let marker = if selected { "› " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(Span::styled(format!("{marker}{}", choice.label()), style))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(menu.kind().title()))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Linha de secção da ajuda.
fn section(title: &str) -> Line<'static> {
    let _span = katu_core::trace_fn!("overlay::section");

    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Linha `nome — ajuda`.
fn entry(name: &str, help: &str) -> Line<'static> {
    let _span = katu_core::trace_fn!("overlay::entry");

    Line::from(vec![
        Span::styled(format!("  {name:<12}"), Style::default().fg(Color::Cyan)),
        Span::raw(help.to_string()),
    ])
}

/// Área centrada com tamanho fixo (clampada ao ecrã).
fn centered(area: Rect, (width, height): (u16, u16)) -> Rect {
    let _span = katu_core::trace_fn!("overlay::centered");

    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    Rect {
        x,
        y,
        width,
        height,
    }
}
