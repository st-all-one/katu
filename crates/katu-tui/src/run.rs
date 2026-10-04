//! Loop de eventos, restauro do terminal (E10-T01) e pintor de streaming (E10-T05).
//!
//! `try_init` liga o modo cru, o ecrã alternativo e um **panic hook** que restaura o terminal; a
//! guarda RAII (`Drop`) restaura em qualquer saída normal. A UI **nunca** bloqueia no kernel
//! (`KERNEL_SURFACE` F2): o loop principal drena o canal de eventos e continua a tratar teclado,
//! rato e resize **enquanto** o turno corre; os efeitos são pedidos por [`Command`] e executados
//! pela borda ([`Handler`]), que emite observação efémera através do [`Painter`].

use std::io;
use std::time::Duration;

use katu_core::diag::{Level, events};
use katu_core::ports::{Cancel, Clock, Flag};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::{DefaultTerminal, restore, try_init};

use crate::Live;
use crate::action::map_key;
use crate::app::App;
use crate::approval::{self, Challenge, ChallengePrompt, ChallengeSignature, Step};
use crate::copy::{self, Selection};
use crate::throttle::{FRAME_INTERVAL_MS, Throttle};
use crate::ui::render;
use crate::{Command, Update};

/// Executor dos efeitos pedidos pela UI (implementado pela borda do binário).
///
/// É **não bloqueante** (`KERNEL_SURFACE` F2): `send` só enfileira o comando e `poll` drena os
/// eventos disponíveis; o loop principal mantém-se livre para tratar input e render.
pub trait Handler {
    /// Enfileira um comando no kernel sem bloquear; devolve eventos imediatos (ex.: erro de envio).
    fn send(&mut self, command: Command) -> Vec<Update>;

    /// Drena os eventos pendentes sem bloquear. `Live`/`ApprovalRequest` passam pelo [`Painter`].
    fn poll(&mut self, painter: &mut Painter<'_>) -> Vec<Update>;

    /// `true` enquanto um comando de trabalho está em curso (o kernel ainda não emitiu `Done`).
    fn busy(&self) -> bool;

    /// Flag de cancelamento partilhada com o kernel (K8): o `Esc`/`Ctrl-C` escreve-a e o kernel lê-a.
    fn cancel_flag(&self) -> Flag {
        Flag::default()
    }
}

/// Período de sondagem de eventos (ms): mantém o CPU baixo sem parecer travado.
const POLL_MILLIS: u64 = 50;

/// Pintor do painel de atividade durante um turno (E10-T05).
///
/// Recebe só eventos **efémeros** e redesenha no terminal (governado pelo [`Throttle`], E10-T03);
/// nunca escreve no log nem no transcript. É também a única peça da borda que toca no terminal para
/// o **challenge-and-response** (E10-T04), que é modal e bloqueia até o humano responder.
pub struct Painter<'a> {
    app: &'a mut App,
    terminal: &'a mut DefaultTerminal,
    throttle: &'a Throttle<'a>,
    error: Option<io::Error>,
    cancel: Flag,
}

impl<'a> Painter<'a> {
    /// Constrói o pintor sobre o estado, o terminal e a flag de cancelamento partilhada.
    pub(crate) fn new(
        app: &'a mut App,
        terminal: &'a mut DefaultTerminal,
        throttle: &'a Throttle<'a>,
        cancel: Flag,
    ) -> Self {
        let _span = katu_core::trace_fn!("run::painter::new");

        Self {
            app,
            terminal,
            throttle,
            error: None,
            cancel,
        }
    }

    /// Regista um evento efémero e redesenha.
    pub fn live(&mut self, live: Live) {
        let _span = katu_core::trace_fn!("run::live");

        self.app.apply_update(Update::Live(live));
        self.redraw();
    }

    /// `true` se o utilizador pediu para **cancelar** o turno (Esc/Ctrl-C durante o stream).
    #[must_use]
    pub fn cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("run::cancelled");

        self.cancel.cancelled()
    }

    /// Cópia da flag de cancelamento (partilhada com o loop do turno, L-P1/L-P3).
    #[must_use]
    pub fn cancel_flag(&self) -> Flag {
        let _span = katu_core::trace_fn!("run::cancel_flag");

        self.cancel.clone()
    }

    /// Toma o erro de desenho acumulado, se houver (a borda decide abortar).
    pub fn take_error(&mut self) -> Option<io::Error> {
        let _span = katu_core::trace_fn!("run::take_error");

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
        let _span = katu_core::trace_fn!("run::challenge");

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
        let _span = katu_core::trace_fn!("run::redraw");

        if self.error.is_some() || !self.throttle.due() {
            return;
        }
        let app: &App = self.app;
        if let Err(error) = self.terminal.draw(|frame| render(frame, app)) {
            self.error = Some(error);
        }
    }
}

/// `true` se a tecla pede o cancelamento do turno: **Esc** ou **Ctrl-C** (E20-T15/L-P1).
fn is_cancel_key(key: KeyEvent) -> bool {
    let _span = katu_core::trace_fn!("run::is_cancel_key");

    matches!(key.code, KeyCode::Esc)
        || (matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Guarda RAII do terminal: restaura em qualquer saída (o `panic` já é coberto pelo hook).
struct TerminalGuard {
    terminal: DefaultTerminal,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let _span = katu_core::trace_fn!("run::enter");

        let terminal = try_init()?;
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _span = katu_core::trace_fn!("run::drop");

        drop(execute!(io::stdout(), DisableMouseCapture));
        restore();
    }
}

/// Corre a UI até o utilizador sair; o handler executa os efeitos.
///
/// O desenho é governado pelo [`Throttle`] (E10-T03): quadros coalescidos por [`Clock`], forçados
/// em cada tecla e fim de turno. O `ratatui` aplica o **diff** das células entre quadros. O loop
/// **nunca** bloqueia no kernel: cada iteração drena os eventos pendentes e volta a sondar o
/// terminal, pelo que rato/resize/cópia continuam a funcionar durante um turno (G7).
///
/// # Errors
/// [`io::Error`] em falha de terminal (setup, desenho ou leitura de eventos).
pub fn run<H: Handler>(mut app: App, handler: &mut H, clock: &dyn Clock) -> io::Result<()> {
    let _span = katu_core::trace_fn!("run::run");

    let mut guard = TerminalGuard::enter()?;
    let throttle = Throttle::new(clock, FRAME_INTERVAL_MS);
    let mut selection = Selection::default();
    loop {
        // 1. Drena o kernel **sem bloquear** (K5): a superfície fica sempre livre.
        let mut painter = Painter::new(
            &mut app,
            &mut guard.terminal,
            &throttle,
            handler.cancel_flag(),
        );
        let updates = handler.poll(&mut painter);
        let error = painter.take_error();
        drop(painter);
        if let Some(error) = error {
            return Err(error);
        }
        for update in updates {
            app.apply_update(update);
        }

        // 2. Redesenha se o orçamento o permitir.
        if throttle.due() {
            guard.terminal.draw(|frame| render(frame, &app))?;
        }

        // 3. Sonda o terminal: teclado, rato e resize (G7).
        if event::poll(Duration::from_millis(POLL_MILLIS))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(key, &mut app, handler, &mut guard.terminal)?;
                }
                Event::Mouse(mouse) => handle_mouse(mouse, &mut selection, &mut guard.terminal)?,
                Event::Resize(_, _) => throttle.request(),
                _ => {}
            }
        }
        if app.should_quit() {
            break;
        }
    }
    Ok(())
}

/// Trata uma tecla: durante um turno alimenta o cancelamento/*steering*; em repouso, o keymap.
fn handle_key<H: Handler>(
    key: KeyEvent,
    app: &mut App,
    handler: &mut H,
    terminal: &mut DefaultTerminal,
) -> io::Result<()> {
    let _span = katu_core::trace_fn!("run::handle_key");

    if handler.busy() {
        turn_key(key, app, handler);
        return Ok(());
    }
    let Some(action) = map_key(key, app.mode()) else {
        return Ok(());
    };
    katu_core::event!(Level::Trace, events::TUI_INPUT);
    let command = app.apply_action(action);
    if let Some(command) = command {
        // Desenha já o estado (mensagem do utilizador + `a trabalhar…`) antes de o kernel
        // responder: sem isto, o ecrã fica no quadro anterior até ao primeiro delta do modelo.
        terminal.draw(|frame| render(frame, app))?;
        let updates = handler.send(command);
        for update in updates {
            app.apply_update(update);
        }
    }
    Ok(())
}

/// Trata uma tecla **durante** um turno: `Esc`/`Ctrl-C` cancelam (L-P1), o resto alimenta o
/// *steering* (E20-T16), que `Enter` envia como [`Command::Steer`].
fn turn_key<H: Handler>(key: KeyEvent, app: &mut App, handler: &mut H) {
    let _span = katu_core::trace_fn!("run::turn_key");

    if is_cancel_key(key) {
        let flag = handler.cancel_flag();
        if !flag.cancelled() {
            flag.request();
            katu_core::event!(Level::Info, events::TUI_CANCEL);
        }
        return;
    }
    match key.code {
        KeyCode::Enter => {
            let prompt = app.steering().trim().to_string();
            if !prompt.is_empty() {
                let _sent = handler.send(Command::Steer(prompt));
                app.set_steering("");
            }
        }
        KeyCode::Backspace => {
            let mut buffer = app.steering().to_string();
            buffer.pop();
            app.set_steering(&buffer);
        }
        KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            let mut buffer = app.steering().to_string();
            buffer.push(character);
            app.set_steering(&buffer);
        }
        _ => {}
    }
}

/// Trata um evento de rato: seleção e cópia por OSC 52 (E20-T14).
fn handle_mouse(
    mouse: MouseEvent,
    selection: &mut Selection,
    terminal: &mut DefaultTerminal,
) -> io::Result<()> {
    let _span = katu_core::trace_fn!("run::handle_mouse");

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
