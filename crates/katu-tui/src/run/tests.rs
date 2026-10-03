//! Testes do mapa de teclas do turno (E20-T15/L-P1): `Esc` **e** `Ctrl-C` cancelam.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::is_cancel_key;

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
