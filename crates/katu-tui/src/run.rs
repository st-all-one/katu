//! Loop de eventos, restauro do terminal (E10-T01) e pintor de streaming (E10-T05).
//!
//! `try_init` liga o modo cru, o ecrã alternativo e um **panic hook** que restaura o terminal; a
//! guarda RAII (`Drop`) restaura em qualquer saída normal. A UI nunca bloqueia o render: os efeitos
//! são pedidos por [`Command`] e executados pela borda ([`Handler`]), que pode emitir observação
//! efémera através do [`Painter`] enquanto o turno corre.

use std::io;
use std::time::Duration;

use katu_core::diag::{Level, events};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::{DefaultTerminal, restore, try_init};

use crate::action::map_key;
use crate::app::{App, Command, Live, Update};
use crate::ui::render;

/// Executor dos efeitos pedidos pela UI (implementado pela borda do binário).
pub trait Handler {
    /// Executa o comando; pode emitir observação incremental via [`Painter`] e devolve as
    /// atualizações finais a injetar no [`App`].
    fn handle(&mut self, command: Command, painter: &mut Painter<'_>) -> Vec<Update>;
}

/// Período de sondagem de eventos (ms): mantém o CPU baixo sem parecer travado.
const POLL_MILLIS: u64 = 50;

/// Pintor do painel de atividade durante um turno (E10-T05).
///
/// Recebe só eventos **efémeros** e redesenha no terminal; nunca escreve no log nem no transcript.
pub struct Painter<'a> {
    app: &'a mut App,
    terminal: &'a mut DefaultTerminal,
    error: Option<io::Error>,
}

impl Painter<'_> {
    /// Regista um evento efémero e redesenha.
    pub fn live(&mut self, live: Live) {
        self.app.apply_update(Update::Live(live));
        self.redraw();
    }

    /// Toma o erro de desenho acumulado, se houver (a borda decide abortar).
    pub fn take_error(&mut self) -> Option<io::Error> {
        self.error.take()
    }

    /// Redesenha se não houver erro pendente (o throttle por tempo é E10-T03).
    fn redraw(&mut self) {
        if self.error.is_some() {
            return;
        }
        let app: &App = self.app;
        if let Err(error) = self.terminal.draw(|frame| render(frame, app)) {
            self.error = Some(error);
        }
    }
}

/// Guarda RAII do terminal: restaura em qualquer saída (o `panic` já é coberto pelo hook).
struct TerminalGuard {
    terminal: DefaultTerminal,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        Ok(Self {
            terminal: try_init()?,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Corre a UI até o utilizador sair; o handler executa os efeitos.
///
/// # Errors
/// [`io::Error`] em falha de terminal (setup, desenho ou leitura de eventos).
pub fn run<H: Handler>(mut app: App, handler: &mut H) -> io::Result<()> {
    let mut guard = TerminalGuard::enter()?;
    loop {
        guard.terminal.draw(|frame| render(frame, &app))?;
        if event::poll(Duration::from_millis(POLL_MILLIS))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = map_key(key, app.mode())
        {
            katu_core::event!(Level::Trace, events::TUI_INPUT);
            if let Some(command) = app.apply_action(action) {
                let mut painter = Painter {
                    app: &mut app,
                    terminal: &mut guard.terminal,
                    error: None,
                };
                let updates = handler.handle(command, &mut painter);
                if let Some(error) = painter.take_error() {
                    return Err(error);
                }
                for update in updates {
                    app.apply_update(update);
                }
            }
        }
        if app.should_quit() {
            break;
        }
    }
    Ok(())
}
