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
    app.apply_update(Update::Live(Live::Tool {
        name: "grep".to_string(),
        args: "{}".to_string(),
    }));
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
fn usage_and_cancellation_updates_are_visible() {
    let mut app = App::new();
    app.apply_update(Update::Usage("tokens in 10 out 5".to_string()));
    assert_eq!(app.usage(), Some("tokens in 10 out 5"));
    app.apply_update(Update::Cancelled);
    assert!(
        matches!(app.status(), Status::Message(_)),
        "o cancelamento fica visível na barra"
    );
}

#[test]
fn done_clears_the_live_panel() {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Text("parcial".to_string())));
    app.apply_update(Update::Live(Live::Tool {
        name: "read".to_string(),
        args: "{}".to_string(),
    }));
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

/// Escreve e submete um comando `/` na linha de mensagem.
fn slash(app: &mut App, command: &str) {
    app.apply_action(Action::StartCommand);
    for character in command.chars() {
        app.apply_action(Action::Insert(character));
    }
    app.apply_action(Action::Submit);
}

#[test]
fn model_menu_selects_a_model() {
    let mut app = App::new();
    app.apply_update(Update::Models(vec!["a".to_string(), "b".to_string()]));
    assert_eq!(app.model(), Some("a"));
    slash(&mut app, "model");
    assert_eq!(app.mode(), Mode::Menu);
    assert!(app.menu().is_some());
    app.apply_action(Action::MenuDown);
    assert_eq!(
        app.apply_action(Action::MenuConfirm),
        Some(Command::SetModel("b".to_string()))
    );
    assert_eq!(app.model(), Some("b"));
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn model_menu_without_a_list_is_reported() {
    let mut app = App::new();
    slash(&mut app, "model");
    assert_eq!(app.mode(), Mode::Normal, "sem modelos não abre menu");
    assert!(matches!(app.status(), Status::Message(_)));
}

#[test]
fn thinking_menu_offers_only_supported_grades() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::ThinkingOptions(vec![Thinking::Off]));
    slash(&mut app, "thinking");
    assert_eq!(app.mode(), Mode::Menu);
    let Some(menu) = app.menu() else {
        return Err("menu devia estar aberto".into());
    };
    assert_eq!(menu.items().len(), 1, "só `off` é suportado");
    assert_eq!(
        app.apply_action(Action::MenuConfirm),
        Some(Command::SetThinking(Thinking::Off))
    );
    Ok(())
}

#[test]
fn changing_model_auto_opens_the_thinking_menu() {
    let mut app = App::new();
    app.apply_update(Update::Models(vec!["a".to_string(), "b".to_string()]));
    app.apply_update(Update::ThinkingOptions(vec![Thinking::Off]));
    slash(&mut app, "model");
    app.apply_action(Action::MenuDown);
    assert_eq!(
        app.apply_action(Action::MenuConfirm),
        Some(Command::SetModel("b".to_string()))
    );
    app.apply_update(Update::ThinkingOptions(vec![Thinking::Off, Thinking::Low]));
    assert_eq!(app.mode(), Mode::Menu, "o submenu de thinking abre sozinho");
    assert!(app.menu().is_some());
}

#[test]
fn help_overlay_opens_and_closes() {
    let mut app = App::new();
    assert_eq!(app.apply_action(Action::OpenHelp), None);
    assert!(app.help_open());
    app.apply_action(Action::CloseOverlay);
    assert!(!app.help_open());
}

#[test]
fn unknown_slash_command_is_reported_not_submitted() {
    let mut app = App::new();
    slash(&mut app, "naoexiste");
    assert_eq!(app.mode(), Mode::Normal);
    assert!(matches!(app.status(), Status::Failure(_)));
    assert!(app.transcript().is_empty(), "não vira mensagem");
}

#[test]
fn shell_prefix_emits_a_shell_command() {
    let mut app = App::new();
    app.apply_action(Action::EnterInsert);
    for character in "!echo oi".chars() {
        app.apply_action(Action::Insert(character));
    }
    assert_eq!(
        app.apply_action(Action::Submit),
        Some(Command::Shell("echo oi".to_string()))
    );
    assert_eq!(app.status(), &Status::Working);
    assert_eq!(app.transcript().first().map(|e| e.role), Some(Role::User));
}

#[test]
fn slash_plan_toggles_and_the_border_confirms() {
    let mut app = App::new();
    app.apply_action(Action::StartCommand);
    for character in "plan".chars() {
        app.apply_action(Action::Insert(character));
    }
    assert_eq!(app.apply_action(Action::Submit), Some(Command::Plan));
    assert!(!app.plan_mode(), "só a borda confirma o estado");
    app.apply_update(Update::Plan(true));
    assert!(app.plan_mode());
    app.apply_update(Update::Plan(false));
    assert!(!app.plan_mode());
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

#[test]
fn emptying_the_trash_emits_a_command() {
    let mut app = App::new();
    app.apply_action(Action::OpenTrash);
    assert_eq!(
        app.apply_action(Action::EmptyTrash),
        Some(Command::EmptyTrash)
    );
}

#[test]
fn next_action_update_fills_the_header_state() {
    let mut app = App::new();
    assert_eq!(app.next_action(), None);
    app.apply_update(Update::NextAction("verificar".to_string()));
    assert_eq!(app.next_action(), Some("verificar"));
}

#[test]
fn opening_the_transcript_emits_a_command_and_fills_the_viewer() {
    let mut app = App::new();
    assert_eq!(
        app.apply_action(Action::OpenTranscript),
        Some(Command::Transcript)
    );
    assert!(app.viewer_open());
    assert_eq!(app.mode(), Mode::Transcript);
    app.apply_update(Update::Transcript(vec![
        "# katu".to_string(),
        "olá".to_string(),
    ]));
    assert_eq!(app.viewer_lines().len(), 2);
    assert_eq!(app.viewer_scroll(), 0);
    app.apply_action(Action::TranscriptDown);
    assert_eq!(app.viewer_scroll(), 1);
    app.apply_action(Action::CloseOverlay);
    assert!(!app.viewer_open());
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn citation_prefixes_the_next_submission() {
    let mut app = App::new();
    app.apply_action(Action::EnterInsert);
    for character in "@src/main.rs".chars() {
        app.apply_action(Action::Insert(character));
    }
    assert_eq!(app.apply_action(Action::Submit), None, "citar não submete");
    assert_eq!(
        app.citations().first().map(String::as_str),
        Some("@src/main.rs")
    );
    app.apply_action(Action::EnterInsert);
    for character in "vê isto".chars() {
        app.apply_action(Action::Insert(character));
    }
    let command = app.apply_action(Action::Submit);
    assert_eq!(
        command,
        Some(Command::Submit("@src/main.rs\n\nvê isto".to_string()))
    );
    assert!(app.citations().is_empty(), "as citações são consumidas");
}
