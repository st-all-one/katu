//! Testes do loop da UI (`KERNEL_SURFACE` F2): cancelamento/*steering* continuam a funcionar
//! durante um turno (a superfície fica livre; G7/G1).

use katu_core::ports::{Cancel, Flag};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Handler, Painter, is_cancel_key, turn_key};
use crate::app::App;
use crate::{Command, Update};

/// Borda mínima: registra os comandos enviados e expõe a flag partilhada.
#[derive(Default)]
struct Mock {
    sent: Vec<Command>,
    flag: Flag,
}

impl Handler for Mock {
    fn send(&mut self, command: Command) -> Vec<Update> {
        self.sent.push(command);
        Vec::new()
    }

    fn poll(&mut self, _painter: &mut Painter<'_>) -> Vec<Update> {
        Vec::new()
    }

    fn busy(&self) -> bool {
        false
    }

    fn cancel_flag(&self) -> Flag {
        self.flag.clone()
    }
}

#[test]
fn esc_and_ctrl_c_cancel_the_round() {
    assert!(is_cancel_key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE
    )));
    assert!(
        is_cancel_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        "Ctrl-C cancela a rodada (L-P1)"
    );
    assert!(!is_cancel_key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::NONE
    )));
    assert!(
        !is_cancel_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        "um `c` normal alimenta o steering, não cancela"
    );
}

#[test]
fn a_cancel_key_writes_the_shared_flag() {
    let mut app = App::new();
    let mut handler = Mock::default();
    turn_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut app,
        &mut handler,
    );
    assert!(handler.flag.cancelled(), "o kernel lê a flag partilhada");
    assert!(handler.sent.is_empty(), "cancelar não envia um comando");
}

#[test]
fn steering_is_typed_then_sent() {
    let mut app = App::new();
    let mut handler = Mock::default();
    for character in ['v', 'a', 'i'] {
        turn_key(
            KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            &mut app,
            &mut handler,
        );
    }
    assert_eq!(app.steering(), "vai");
    turn_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut app,
        &mut handler,
    );
    assert_eq!(app.steering(), "", "o buffer é limpo ao enviar");
    assert!(matches!(
        handler.sent.last(),
        Some(Command::Steer(text)) if text == "vai"
    ));
}
