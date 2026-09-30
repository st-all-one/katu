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
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::{DefaultTerminal, restore, try_init};

use crate::action::map_key;
use crate::app::App;
use crate::approval::{self, Challenge, ChallengePrompt, ChallengeSignature, Step};
use crate::copy::{self, Selection};
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
    cancel: bool,
    /// Prompts de *steering* enfileirados, por ordem (FIFO; E20-T16).
    steer: Vec<String>,
    /// Buffer de *steering* em edição durante o turno.
    steer_buffer: String,
}

impl Painter<'_> {
    /// Regista um evento efémero e redesenha.
    pub fn live(&mut self, live: Live) {
        self.poll_input();
        self.app.apply_update(Update::Live(live));
        self.redraw();
    }

    /// `true` se o utilizador pediu para **cancelar** o turno (Esc durante o stream).
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.cancel
    }

    /// Retira o próximo prompt de *steering* enfileirado, se houver (E20-T16).
    #[must_use]
    pub fn take_steer(&mut self) -> Option<String> {
        if self.steer.is_empty() {
            return None;
        }
        Some(self.steer.remove(0))
    }

    /// Sonda o teclado (não bloqueante) durante o turno.
    ///
    /// **Esc** pede cancelamento (E20-T15); as restantes teclas alimentam o buffer de *steering*
    /// (E20-T16), que `Enter` enfileira e a borda aplica no passo seguinte. `Ctrl-C`/`q` saem da UI
    /// no loop principal (não aqui).
    fn poll_input(&mut self) {
        loop {
            match event::poll(Duration::ZERO) {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
            match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => self.on_key(key),
                Ok(_) => {}
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
    }

    /// Trata uma tecla durante o turno (cancelamento ou *steering*).
    fn on_key(&mut self, key: KeyEvent) {
        if is_cancel_key(key) {
            if !self.cancel {
                self.cancel = true;
                katu_core::event!(Level::Info, events::TUI_CANCEL);
            }
            return;
        }
        match key.code {
            KeyCode::Enter => {
                let prompt = self.steer_buffer.trim().to_string();
                if !prompt.is_empty() {
                    self.steer.push(prompt);
                    self.steer_buffer.clear();
                    self.app.set_steering("");
                }
            }
            KeyCode::Backspace => {
                self.steer_buffer.pop();
                let buffer = self.steer_buffer.clone();
                self.app.set_steering(&buffer);
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.steer_buffer.push(character);
                let buffer = self.steer_buffer.clone();
                self.app.set_steering(&buffer);
            }
            _ => {}
        }
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

/// `true` se a tecla pede o cancelamento do turno: **só `Esc`** (E20-T15).
fn is_cancel_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
}

/// Guarda RAII do terminal: restaura em qualquer saída (o `panic` já é coberto pelo hook).
struct TerminalGuard {
    terminal: DefaultTerminal,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let terminal = try_init()?;
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        drop(execute!(io::stdout(), DisableMouseCapture));
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
    let mut selection = Selection::default();
    loop {
        if throttle.due() {
            guard.terminal.draw(|frame| render(frame, &app))?;
        }
        if event::poll(Duration::from_millis(POLL_MILLIS))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Some(action) = map_key(key, app.mode()) {
                        katu_core::event!(Level::Trace, events::TUI_INPUT);
                        let command = app.apply_action(action);
                        throttle.request();
                        if let Some(command) = command {
                            let mut painter = Painter {
                                app: &mut app,
                                terminal: &mut guard.terminal,
                                throttle: &throttle,
                                error: None,
                                cancel: false,
                                steer: Vec::new(),
                                steer_buffer: String::new(),
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
                }
                Event::Mouse(mouse) => handle_mouse(mouse, &mut selection, &mut guard.terminal)?,
                _ => {}
            }
        }
        if app.should_quit() {
            break;
        }
    }
    Ok(())
}

/// Trata um evento de rato: seleção e cópia por OSC 52 (E20-T14).
fn handle_mouse(
    mouse: MouseEvent,
    selection: &mut Selection,
    terminal: &mut DefaultTerminal,
) -> io::Result<()> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => selection.start((mouse.column, mouse.row)),
        MouseEventKind::Drag(MouseButton::Left) => selection.drag((mouse.column, mouse.row)),
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some((start, end)) = selection.take() {
                let text = copy::extract(terminal.current_buffer_mut(), start, end);
                copy::osc52(&text)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
