//! Testes do mapa de teclas do turno (E20-T15): só `Esc` cancela.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::is_cancel_key;

#[test]
fn only_esc_cancels_the_round() {
    assert!(is_cancel_key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE
    )));
    assert!(
        !is_cancel_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        "Ctrl-C sai da UI, não cancela a rodada"
    );
    assert!(!is_cancel_key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::NONE
    )));
}
