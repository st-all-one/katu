//! Loop de eventos, restauro do terminal (E10-T01) e pintor de streaming (E10-T05).
//!
//! `try_init` liga o modo cru, o ecrã alternativo e um **panic hook** que restaura o terminal; a
//! guarda RAII (`Drop`) restaura em qualquer saída normal. A UI nunca bloqueia o render: os efeitos
//! são pedidos por [`Command`] e executados pela borda ([`Handler`]), que pode emitir observação
//! efémera através do [`Painter`] enquanto o turno corre.

use std::io;
use std::time::Duration;

use katu_core::diag::{Level, events};
use katu_core::ports::Clock;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::{DefaultTerminal, restore, try_init};

use crate::action::map_key;
use crate::app::App;
use crate::approval::{self, Challenge, ChallengePrompt, ChallengeSignature, Step};
use crate::live::Live;
use crate::throttle::{FRAME_INTERVAL_MS, Throttle};
use crate::ui::render;
use crate::{Command, Update};

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
/// Recebe só eventos **efémeros** e redesenha no terminal (governado pelo [`Throttle`], E10-T03);
/// nunca escreve no log nem no transcript.
pub struct Painter<'a> {
    app: &'a mut App,
    terminal: &'a mut DefaultTerminal,
    throttle: &'a Throttle<'a>,
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

    /// Apresenta o **challenge-and-response** e devolve a assinatura (E10-T04, §33).
    ///
    /// Bloqueia o turno até o humano responder; `Esc`/`Ctrl-C` cancelam (fail-closed → `None`).
    pub fn challenge(
        &mut self,
        prompt: ChallengePrompt,
        granted_by: &str,
    ) -> Option<ChallengeSignature> {
        let mut challenge = Challenge::new(prompt);
        loop {
            if self.error.is_some() {
                return None;
            }
            let app: &App = self.app;
            let drawn = self.terminal.draw(|frame| {
                render(frame, app);
                approval::render(frame, &challenge);
            });
            if let Err(error) = drawn {
                self.error = Some(error);
                return None;
            }
            let key = match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => key,
                Ok(_) => continue,
                Err(error) => {
                    self.error = Some(error);
                    return None;
                }
            };
            let Some(key) = approval::map_key(key) else {
                continue;
            };
            match challenge.apply(key) {
                Step::Continue => {}
                Step::Approved => return challenge.signature(granted_by),
                Step::Cancelled => return None,
            }
        }
    }

    /// Redesenha se o orçamento de render o permitir (E10-T03) e não houver erro pendente.
    fn redraw(&mut self) {
        if self.error.is_some() || !self.throttle.due() {
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
/// O desenho é governado pelo [`Throttle`] (E10-T03): quadros coalescidos por [`Clock`], forçados
/// em cada tecla e fim de turno. O `ratatui` aplica o **diff** das células entre quadros.
///
/// # Errors
/// [`io::Error`] em falha de terminal (setup, desenho ou leitura de eventos).
pub fn run<H: Handler>(mut app: App, handler: &mut H, clock: &dyn Clock) -> io::Result<()> {
    let mut guard = TerminalGuard::enter()?;
    let throttle = Throttle::new(clock, FRAME_INTERVAL_MS);
    loop {
        if throttle.due() {
            guard.terminal.draw(|frame| render(frame, &app))?;
        }
        if event::poll(Duration::from_millis(POLL_MILLIS))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = map_key(key, app.mode())
        {
            katu_core::event!(Level::Trace, events::TUI_INPUT);
            let command = app.apply_action(action);
            throttle.request();
            if let Some(command) = command {
                let mut painter = Painter {
                    app: &mut app,
                    terminal: &mut guard.terminal,
                    throttle: &throttle,
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
