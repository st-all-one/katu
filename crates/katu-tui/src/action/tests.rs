//! Testes do keymap puro por modo (E10-T02/E20-T10).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Mode, map_key};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

#[test]
fn normal_mode_navigates_and_opens_surfaces() {
    assert_eq!(
        map_key(key(KeyCode::Char('q')), Mode::Normal),
        Some(Action::Quit)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('i')), Mode::Normal),
        Some(Action::EnterInsert)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('/')), Mode::Normal),
        Some(Action::StartCommand)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('?')), Mode::Normal),
        Some(Action::OpenHelp)
    );
    assert_eq!(
        map_key(key(KeyCode::Up), Mode::Normal),
        Some(Action::ScrollUp)
    );
    assert_eq!(
        map_key(key(KeyCode::Down), Mode::Normal),
        Some(Action::ScrollDown)
    );
}

#[test]
fn old_one_touch_shortcuts_are_gone() {
    for character in ['m', 't', 'l', 'T', 'c', 'v'] {
        assert_eq!(
            map_key(key(KeyCode::Char(character)), Mode::Normal),
            None,
            "{character} foi delegado em `/`"
        );
    }
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
fn help_overlay_closes_with_esc_q_or_question() {
    for code in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Char('?')] {
        assert_eq!(
            map_key(key(code), Mode::Help),
            Some(Action::CloseOverlay),
            "{code:?}"
        );
    }
}

#[test]
fn menu_navigates_and_confirms() {
    assert_eq!(map_key(key(KeyCode::Up), Mode::Menu), Some(Action::MenuUp));
    assert_eq!(
        map_key(key(KeyCode::Down), Mode::Menu),
        Some(Action::MenuDown)
    );
    assert_eq!(
        map_key(key(KeyCode::Enter), Mode::Menu),
        Some(Action::MenuConfirm)
    );
    assert_eq!(
        map_key(key(KeyCode::Esc), Mode::Menu),
        Some(Action::CloseOverlay)
    );
}

#[test]
fn trash_mode_navigates_and_restores() {
    assert_eq!(
        map_key(key(KeyCode::Char('r')), Mode::Trash),
        Some(Action::Restore)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('x')), Mode::Trash),
        Some(Action::EmptyTrash)
    );
    assert_eq!(
        map_key(key(KeyCode::Up), Mode::Trash),
        Some(Action::TrashUp)
    );
    assert_eq!(
        map_key(key(KeyCode::Down), Mode::Trash),
        Some(Action::TrashDown)
    );
    assert_eq!(
        map_key(key(KeyCode::Esc), Mode::Trash),
        Some(Action::CloseOverlay)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('q')), Mode::Trash),
        Some(Action::CloseOverlay),
        "q fecha a sobreposição, não a UI"
    );
}

#[test]
fn transcript_mode_scrolls_and_closes() {
    assert_eq!(
        map_key(key(KeyCode::Up), Mode::Transcript),
        Some(Action::TranscriptUp)
    );
    assert_eq!(
        map_key(key(KeyCode::Down), Mode::Transcript),
        Some(Action::TranscriptDown)
    );
    assert_eq!(
        map_key(key(KeyCode::Esc), Mode::Transcript),
        Some(Action::CloseOverlay)
    );
    assert_eq!(
        map_key(key(KeyCode::Char('q')), Mode::Transcript),
        Some(Action::CloseOverlay),
        "q fecha a vista, não a UI"
    );
}

#[test]
fn control_c_quits_in_every_mode() {
    for mode in [
        Mode::Normal,
        Mode::Insert,
        Mode::Confirm,
        Mode::Trash,
        Mode::Transcript,
        Mode::Help,
        Mode::Menu,
    ] {
        assert_eq!(map_key(ctrl('c'), mode), Some(Action::Quit), "{mode:?}");
    }
}

#[test]
fn control_char_is_not_inserted() {
    assert_eq!(map_key(ctrl('a'), Mode::Insert), None);
}
