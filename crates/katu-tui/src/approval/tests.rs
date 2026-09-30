use super::{Challenge, ChallengePrompt, Focus, Key, QUESTIONS, Step, map_key};

fn challenge() -> Challenge {
    Challenge::new(ChallengePrompt {
        tool: "read".to_string(),
        rule: "contain-sensitive-read".to_string(),
        scope: "/home/ana/.ssh/id_rsa".to_string(),
    })
}

#[test]
fn escape_and_ctrl_c_cancel() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    assert_eq!(
        map_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(Key::Cancel)
    );
    assert_eq!(
        map_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Some(Key::Cancel)
    );
    assert_eq!(
        map_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        Some(Key::Char('a'))
    );
}

#[test]
fn is_not_complete_until_all_questions_and_a_reason() {
    let mut challenge = challenge();
    for _ in QUESTIONS {
        assert_eq!(challenge.apply(Key::Toggle), Step::Continue);
        assert_eq!(challenge.apply(Key::Submit), Step::Continue);
        if challenge.cursor() < QUESTIONS.len().saturating_sub(1) {
            challenge.apply(Key::Down);
        }
    }
    assert!(!challenge.is_complete(), "falta a justificação");
    challenge.apply(Key::Tab);
    assert_eq!(challenge.focus(), Focus::Reason);
    challenge.apply(Key::Char('p'));
    challenge.apply(Key::Char('o'));
    assert!(challenge.is_complete());
    assert_eq!(challenge.apply(Key::Submit), Step::Approved);
}

#[test]
fn submit_without_answers_stays_pending() {
    let mut challenge = challenge();
    assert_eq!(challenge.apply(Key::Submit), Step::Continue);
    assert!(challenge.signature("ana").is_none());
}

#[test]
fn cancel_is_fail_closed() {
    let mut challenge = challenge();
    assert_eq!(challenge.apply(Key::Cancel), Step::Cancelled);
}

#[test]
fn signature_carries_the_reason_and_the_signer() -> Result<(), Box<dyn std::error::Error>> {
    let mut challenge = challenge();
    for index in 0..QUESTIONS.len() {
        challenge.apply(Key::Toggle);
        if index.saturating_add(1) < QUESTIONS.len() {
            challenge.apply(Key::Down);
        }
    }
    challenge.apply(Key::Tab);
    for c in "revisão manual".chars() {
        challenge.apply(Key::Char(c));
    }
    let signature = challenge.signature("ana").ok_or("challenge incompleto")?;
    assert_eq!(signature.reason, "revisão manual");
    assert_eq!(signature.granted_by, "ana");
    Ok(())
}
