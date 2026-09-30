//! Testes do estado central da UI (E10-T02/T05/T07) pela API pública.

use katu_core::provider::Thinking;
use katu_tui::{Action, App, Command, Live, Mode, Role, Status, TrashEntry, Update};

#[test]
fn typing_then_submitting_emits_a_command_and_echoes_the_user() {
    let mut app = App::new();
    app.apply_action(Action::EnterInsert);
    for character in "olá".chars() {
        app.apply_action(Action::Insert(character));
    }
    assert_eq!(app.input(), "olá");
    let command = app.apply_action(Action::Submit);
    assert_eq!(command, Some(Command::Submit("olá".to_string())));
    assert!(app.input().is_empty());
    assert!(app.pending());
    assert_eq!(app.status(), &Status::Working);
    assert_eq!(app.transcript().first().map(|e| e.role), Some(Role::User));
}

#[test]
fn empty_submit_is_a_noop() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::Submit), None);
    assert!(!app.pending());
}

#[test]
fn updates_fill_the_transcript_and_clear_pending() {
    let mut app = App::new();
    app.apply_action(Action::EnterInsert);
    app.apply_action(Action::Insert('x'));
    app.apply_action(Action::Submit);
    app.apply_update(Update::Assistant("feito".to_string()));
    app.apply_update(Update::Tool("read".to_string()));
    app.apply_update(Update::Phase("implemented".to_string()));
    app.apply_update(Update::Done);
    assert!(!app.pending());
    assert_eq!(app.phase(), "implemented");
    assert_eq!(app.transcript().len(), 3);
    assert_eq!(app.status(), &Status::Idle);
}

#[test]
fn quit_sets_the_flag() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::Quit), Some(Command::Quit));
    assert!(app.should_quit());
}

#[test]
fn live_stream_accumulates_outside_the_transcript() {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Text("olá ".to_string())));
    app.apply_update(Update::Live(Live::Text("mundo".to_string())));
    app.apply_update(Update::Live(Live::Thinking("hmm".to_string())));
    app.apply_update(Update::Live(Live::Tool("grep".to_string())));
    app.apply_update(Update::Live(Live::ToolDone("grep".to_string())));
    assert_eq!(app.streaming(), "olá mundo");
    assert_eq!(app.thinking(), "hmm");
    assert_eq!(app.live(), ["→ grep".to_string(), "✓ grep".to_string()]);
    assert!(
        app.transcript().is_empty(),
        "o live não entra no transcript"
    );
}

#[test]
fn done_clears_the_live_panel() {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Text("parcial".to_string())));
    app.apply_update(Update::Live(Live::Tool("read".to_string())));
    app.apply_update(Update::Assistant("final".to_string()));
    app.apply_update(Update::Done);
    assert!(app.streaming().is_empty());
    assert!(app.live().is_empty());
    assert!(app.thinking().is_empty());
    assert_eq!(app.transcript().len(), 1);
}

#[test]
fn refusal_is_shown_and_kept_in_the_transcript() {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Refused {
        rule: "contain-sensitive-read".to_string(),
        evidence: ".env".to_string(),
    }));
    app.apply_update(Update::Live(Live::Unavailable {
        control: "approval".to_string(),
    }));
    assert!(
        app.live()
            .iter()
            .any(|line| line.contains("contain-sensitive-read")),
        "a recusa aparece no painel"
    );
    assert!(
        app.transcript()
            .iter()
            .any(|entry| entry.role == Role::Error && entry.text.contains(".env")),
        "a recusa fica no transcript como erro"
    );
    app.apply_update(Update::Done);
    assert!(app.live().is_empty(), "o painel limpa no fim do turno");
    assert_eq!(app.transcript().len(), 1, "o transcript mantém a recusa");
}

#[test]
fn cycling_the_model_emits_a_set_model_command() {
    let mut app = App::new();
    app.apply_update(Update::Models(vec!["a".to_string(), "b".to_string()]));
    assert_eq!(app.model(), Some("a"));
    assert_eq!(
        app.apply_action(Action::CycleModel),
        Some(Command::SetModel("b".to_string()))
    );
    assert_eq!(app.model(), Some("b"));
    assert_eq!(
        app.apply_action(Action::CycleModel),
        Some(Command::SetModel("a".to_string())),
        "cicla de volta ao primeiro"
    );
}

#[test]
fn cycling_the_model_without_a_list_is_a_noop() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::CycleModel), None);
    assert_eq!(app.model(), None);
}

#[test]
fn cycling_thinking_advances_and_wraps() {
    let mut app = App::new();
    assert_eq!(app.reasoning(), Thinking::Off);
    assert_eq!(
        app.apply_action(Action::CycleThinking),
        Some(Command::SetThinking(Thinking::Low))
    );
    assert_eq!(app.reasoning(), Thinking::Low);
    for _ in 0..3 {
        app.apply_action(Action::CycleThinking);
    }
    assert_eq!(app.reasoning(), Thinking::Off, "o ciclo fecha");
}

#[test]
fn opening_the_trash_lists_navigates_and_restores() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::OpenTrash), Some(Command::Trash));
    assert!(app.trash_open());
    assert_eq!(app.mode(), Mode::Trash);
    app.apply_update(Update::Trash(vec![
        TrashEntry {
            original: "a".to_string(),
            stored: "/t/a".to_string(),
        },
        TrashEntry {
            original: "b".to_string(),
            stored: "/t/b".to_string(),
        },
    ]));
    assert_eq!(app.trash_index(), 0);
    app.apply_action(Action::TrashDown);
    assert_eq!(app.trash_index(), 1);
    app.apply_action(Action::TrashDown);
    assert_eq!(app.trash_index(), 1, "satura no fim");
    app.apply_action(Action::TrashUp);
    assert_eq!(app.trash_index(), 0);
    assert_eq!(
        app.apply_action(Action::Restore),
        Some(Command::Restore("/t/a".to_string()))
    );
    app.apply_action(Action::CloseOverlay);
    assert!(!app.trash_open());
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn compact_emits_a_command() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::Compact), Some(Command::Compact));
}

#[test]
fn verify_emits_a_command() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::Verify), Some(Command::Verify));
}

#[test]
fn restoring_an_empty_trash_is_a_noop() {
    let mut app = App::new();
    app.apply_action(Action::OpenTrash);
    assert_eq!(app.apply_action(Action::Restore), None);
}
