use super::{Message, derive_messages, snapshot, state_of};
use crate::error::ToolOutcome;
use crate::kernel::Visibility;
use crate::kernel::event::Event;
use crate::kernel::{CallId, State, step};
use katu_policy::Phase;

fn session() -> Vec<Event> {
    vec![
        Event::TurnStart { turn: 1 },
        Event::UserMessage {
            text: "faz isto".into(),
            visibility: Visibility::User,
        },
        Event::AssistantMessage { text: "ok".into() },
        Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "teste de projeção".into(),
        },
        Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        },
        Event::TurnEnd { turn: 1 },
    ]
}

#[test]
fn messages_exclude_control_events() {
    let messages = derive_messages(&session());
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    assert!(matches!(messages.get(1), Some(Message::Assistant { .. })));
}

#[test]
fn state_of_replays_final_state() -> Result<(), Box<dyn std::error::Error>> {
    let events = session();
    let state = state_of(&events)?;
    assert_eq!(state.phase, Phase::KnowledgeConsulted);
    assert_eq!(state.turn, 1);
    assert!(!state.turn_open);

    let mut manual = State::initial();
    for event in &events {
        manual = step(&manual, event)?;
    }
    assert_eq!(state, manual);
    Ok(())
}

#[test]
fn snapshot_counts_visible_messages() -> Result<(), Box<dyn std::error::Error>> {
    let snap = snapshot(&session())?;
    assert_eq!(snap.messages, 2);
    assert_eq!(snap.phase, Phase::KnowledgeConsulted);
    Ok(())
}

#[test]
fn illegal_sequence_is_refused_and_state_unchanged() {
    let events = vec![Event::PhaseTransition {
        to: Phase::Closed,
        outcome: None,
    }];
    let state = State::initial();
    assert!(state_of(&events).is_err());
    if let Some(event) = events.first() {
        assert!(step(&state, event).is_err());
    }
    assert_eq!(state, State::initial());
}

#[test]
fn model_visible_matches_logged_events() {
    let events = session();
    let visible = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::UserMessage { .. }
                    | Event::AssistantMessage { .. }
                    | Event::ToolCall { .. }
                    | Event::ToolResult { .. }
            )
        })
        .count();
    assert_eq!(derive_messages(&events).len(), visible);
}

#[test]
fn orphan_tool_call_is_dropped_from_the_projection() -> Result<(), Box<dyn std::error::Error>> {
    let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
    let events = vec![
        Event::UserMessage {
            text: "lê".into(),
            visibility: Visibility::User,
        },
        Event::ToolCall {
            call: CallId::new("c1"),
            tool: katu_policy::ToolUse {
                name: katu_policy::ToolName::Read,
                args: katu_policy::ToolArgs::Read { path: path.clone() },
                resolved_paths: vec![path.clone()],
                argv: None,
                cwd: path,
            },
        },
    ];
    let messages = derive_messages(&events);
    assert_eq!(
        messages.len(),
        1,
        "a call sem resultado não chega ao modelo"
    );
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    Ok(())
}

#[test]
fn orphan_tool_result_is_dropped_from_the_projection() {
    let events = vec![
        Event::UserMessage {
            text: "lê".into(),
            visibility: Visibility::User,
        },
        Event::ToolResult {
            call: CallId::new("c1"),
            outcome: ToolOutcome::Ok,
            delta: Some("conteúdo".into()),
        },
    ];
    let messages = derive_messages(&events);
    assert_eq!(
        messages.len(),
        1,
        "o resultado sem pedido não chega ao modelo"
    );
    assert!(matches!(messages.first(), Some(Message::User { .. })));
}

#[test]
fn the_projection_starts_at_the_first_user_message() {
    let events = vec![
        Event::AssistantMessage {
            text: "pré-rolo".into(),
        },
        Event::UserMessage {
            text: "olá".into(),
            visibility: Visibility::User,
        },
        Event::AssistantMessage { text: "oi".into() },
    ];
    let messages = derive_messages(&events);
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    assert!(matches!(messages.get(1), Some(Message::Assistant { .. })));
}

#[test]
fn tool_result_carries_the_tool_name_from_the_call() -> Result<(), Box<dyn std::error::Error>> {
    let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
    let events = vec![
        Event::UserMessage {
            text: "lê".into(),
            visibility: Visibility::User,
        },
        Event::ToolCall {
            call: CallId::new("c1"),
            tool: katu_policy::ToolUse {
                name: katu_policy::ToolName::Read,
                args: katu_policy::ToolArgs::Read { path: path.clone() },
                resolved_paths: vec![path.clone()],
                argv: None,
                cwd: path,
            },
        },
        Event::ToolResult {
            call: CallId::new("c1"),
            outcome: ToolOutcome::Ok,
            delta: Some("conteúdo".into()),
        },
    ];
    let messages = derive_messages(&events);
    let Some(Message::ToolResult { tool_name, .. }) = messages.get(2) else {
        return Err("esperado um ToolResult".into());
    };
    assert_eq!(*tool_name, Some(katu_policy::ToolName::Read));
    Ok(())
}

#[test]
fn a_blind_retry_gets_a_fix_that_points_elsewhere() -> Result<(), Box<dyn std::error::Error>> {
    // Cenário canónico: o modelo lê fora da raiz, é negado, e a negação ensina a ler sob a
    // raiz — uma tentativa cega (repetir o mesmo caminho) continuaria a ser negada.
    let rule = katu_policy::RuleId::from("contain-read-outside-workspace");
    let evidence = katu_policy::Evidence::new("fora da raiz", "/etc/passwd", rule)
        .with_remedy(Some("leia só sob a raiz do workspace".to_string()));
    let denied = ToolOutcome::Denied {
        rule_id: katu_policy::RuleId::from("contain-read-outside-workspace"),
        evidence,
    };
    let summary = denied.summary();
    assert!(
        summary.contains("fix: leia só sob a raiz do workspace"),
        "{summary}"
    );
    // O remédio aponta para uma ação **diferente** da que foi negada (ler sob a raiz, não fora).
    let fix = denied.fix().ok_or("esperado um remédio")?;
    assert!(
        !fix.contains("/etc/passwd"),
        "o remédio não deve repetir o caminho negado"
    );
    Ok(())
}

#[test]
fn an_agent_nudge_reaches_the_model() {
    // G3: o nudge é `Agent` — chega ao modelo, mas não é do utilizador.
    let events = vec![
        Event::UserMessage {
            text: "lê".into(),
            visibility: Visibility::User,
        },
        Event::UserMessage {
            text: "nudge".into(),
            visibility: Visibility::Agent,
        },
    ];
    let messages = derive_messages(&events);
    assert_eq!(messages.len(), 2, "o nudge é model-visible");
    assert!(matches!(
        messages.get(1),
        Some(Message::User {
            visibility: Visibility::Agent,
            ..
        })
    ));
}

#[test]
fn an_empty_message_is_dropped() {
    // G6: uma mensagem de texto vazia é ruído para o endpoint.
    let events = vec![
        Event::UserMessage {
            text: "   ".into(),
            visibility: Visibility::User,
        },
        Event::UserMessage {
            text: "olá".into(),
            visibility: Visibility::User,
        },
    ];
    let messages = derive_messages(&events);
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages.first(), Some(Message::User { .. })));
}

#[test]
fn a_tool_result_before_its_call_is_dropped() -> Result<(), Box<dyn std::error::Error>> {
    // G6: um resultado fora de ordem deixa o par inválido — nem a call nem o resultado chegam.
    let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
    let events = vec![
        Event::UserMessage {
            text: "lê".into(),
            visibility: Visibility::User,
        },
        Event::ToolResult {
            call: CallId::new("c1"),
            outcome: ToolOutcome::Ok,
            delta: Some("conteúdo".into()),
        },
        Event::ToolCall {
            call: CallId::new("c1"),
            tool: katu_policy::ToolUse {
                name: katu_policy::ToolName::Read,
                args: katu_policy::ToolArgs::Read { path: path.clone() },
                resolved_paths: vec![path.clone()],
                argv: None,
                cwd: path,
            },
        },
    ];
    let messages = derive_messages(&events);
    assert_eq!(messages.len(), 1, "o par fora de ordem é descartado");
    assert!(matches!(messages.first(), Some(Message::User { .. })));
    Ok(())
}
