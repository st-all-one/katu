//! Keymap **puro** (E10-T02): `map_key(KeyEvent, Mode) -> Option<Action>`.
//!
//! Nenhuma lógica de UI no handler de eventos: o handler só traduz a tecla numa [`Action`] e
//! delega em [`App::apply_action`](crate::App::apply_action). Testável por modo, sem terminal.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Modo de entrada da UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Navegação: atalhos de um toque (`q`, `i`, setas).
    #[default]
    Normal,
    /// Edição da linha de mensagem.
    Insert,
    /// Confirmação de uma ação destrutiva (`y`/`n`).
    Confirm,
}

/// Ação pura produzida pelo teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Entra no modo de edição.
    EnterInsert,
    /// Volta ao modo de navegação.
    LeaveInsert,
    /// Insere um caractere na mensagem.
    Insert(char),
    /// Apaga o último caractere da mensagem.
    Backspace,
    /// Submete a mensagem (efeito na borda).
    Submit,
    /// Cancela a edição/confirmação.
    Cancel,
    /// Confirma a ação pendente.
    Confirm,
    /// Rola a conversa para cima (mais antigo).
    ScrollUp,
    /// Rola a conversa para baixo (mais recente).
    ScrollDown,
    /// Sai da UI.
    Quit,
}

/// Traduz uma tecla na ação correspondente ao modo (função pura, §E10-T02).
#[must_use]
pub fn map_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && matches!(key.code, KeyCode::Char('c')) {
        return Some(Action::Quit);
    }
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Enter | KeyCode::Char('i') => Some(Action::EnterInsert),
            KeyCode::Up => Some(Action::ScrollUp),
            KeyCode::Down => Some(Action::ScrollDown),
            _ => None,
        },
        Mode::Insert => match key.code {
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Esc => Some(Action::LeaveInsert),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(_) if control => None,
            KeyCode::Char(character) => Some(Action::Insert(character)),
            _ => None,
        },
        Mode::Confirm => match key.code {
            KeyCode::Char('y' | 'Y') => Some(Action::Confirm),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Action, Mode, map_key};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    #[test]
    fn normal_mode_has_one_touch_shortcuts() {
        assert_eq!(
            map_key(key(KeyCode::Char('q')), Mode::Normal),
            Some(Action::Quit)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('i')), Mode::Normal),
            Some(Action::EnterInsert)
        );
        assert_eq!(
            map_key(key(KeyCode::Up), Mode::Normal),
            Some(Action::ScrollUp)
        );
        assert_eq!(
            map_key(key(KeyCode::Down), Mode::Normal),
            Some(Action::ScrollDown)
        );
        assert_eq!(map_key(key(KeyCode::Char('x')), Mode::Normal), None);
    }

    #[test]
    fn insert_mode_edits_and_submits() {
        assert_eq!(
            map_key(key(KeyCode::Char('a')), Mode::Insert),
            Some(Action::Insert('a'))
        );
        assert_eq!(
            map_key(key(KeyCode::Enter), Mode::Insert),
            Some(Action::Submit)
        );
        assert_eq!(
            map_key(key(KeyCode::Esc), Mode::Insert),
            Some(Action::LeaveInsert)
        );
        assert_eq!(
            map_key(key(KeyCode::Backspace), Mode::Insert),
            Some(Action::Backspace)
        );
    }

    #[test]
    fn confirm_mode_answers_yes_no() {
        assert_eq!(
            map_key(key(KeyCode::Char('y')), Mode::Confirm),
            Some(Action::Confirm)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('n')), Mode::Confirm),
            Some(Action::Cancel)
        );
        assert_eq!(
            map_key(key(KeyCode::Esc), Mode::Confirm),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn control_c_quits_in_every_mode() {
        for mode in [Mode::Normal, Mode::Insert, Mode::Confirm] {
            assert_eq!(map_key(ctrl('c'), mode), Some(Action::Quit), "{mode:?}");
        }
    }

    #[test]
    fn control_char_is_not_inserted() {
        assert_eq!(map_key(ctrl('a'), Mode::Insert), None);
    }
}
