//! E13-T07 — invariantes de runtime testadas explicitamente.
//!
//! - `Model-visible ⟺ logged`: o histórico é a projeção do log; eventos que não são mensagens não
//!   aparecem e o estado confere com o log (`Session::verify`).
//! - "nunca `Ok` com erros": um evento recusado não muda estado nem log (escrita atómica).

use crate::kernel::Visibility;
use std::path::Path;

use super::{Session, SessionError};
use crate::kernel::Event;
use crate::kernel::log::read_records;
use crate::ports::MemFs;

#[test]
fn model_visible_is_exactly_the_logged_messages() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "olá".into(),
        visibility: Visibility::User,
    })?;
    session.apply(&Event::AssistantMessage { text: "oi".into() })?;
    // Eventos que não são mensagens não entram no histórico.
    session.apply(&Event::TurnEnd { turn: 1 })?;
    assert_eq!(session.messages()?.len(), 2, "só as mensagens são visíveis");
    session.verify()?;
    Ok(())
}

#[test]
fn refused_event_leaves_state_and_log_intact() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    let before = session.state().clone();
    let result = session.apply(&Event::TurnEnd { turn: 9 });
    assert!(matches!(result, Err(SessionError::Refusal(_))));
    assert_eq!(session.state(), &before, "o estado não muda");
    assert!(
        read_records(&fs, session.log_path())?.is_empty(),
        "log vazio"
    );
    Ok(())
}
