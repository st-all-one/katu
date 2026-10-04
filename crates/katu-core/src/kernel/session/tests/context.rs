//! Contexto efetivo do turno com orçamento e compactação (E09-T01/T07), sobre a sessão.

use super::Session;
use crate::context::{CompactionMode, ContextBudget, message_id, recover};
use crate::kernel::Event;
use crate::kernel::Visibility;
use crate::kernel::derive_messages;
use crate::ports::MemFs;
use std::path::Path;

/// Orçamento pequeno para forçar a compactação do prefixo nos testes.
const BUDGET: ContextBudget = ContextBudget {
    raw_min: 25,
    summary_max: 256,
};

#[test]
fn context_keeps_the_recent_suffix_and_compacts_only_when_enabled()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    for index in 0..6_u32 {
        session.apply(&Event::UserMessage {
            text: format!("mensagem numero {index} com texto suficiente"),
            visibility: Visibility::User,
        })?;
        session.apply(&Event::AssistantMessage {
            text: format!("resposta numero {index}"),
        })?;
    }

    let plain = session.context(BUDGET, CompactionMode::Disabled)?;
    assert!(plain.summary.is_none(), "desligado não resume nada");
    assert!(plain.messages.len() < 12, "o assemble corta o prefixo");

    let compacted = session.context(BUDGET, CompactionMode::Enabled)?;
    assert!(
        compacted.summary.as_deref().is_some_and(|s| !s.is_empty()),
        "ligado preenche o digest"
    );
    assert_eq!(compacted.messages, plain.messages, "o sufixo cru é o mesmo");
    assert_eq!(
        compacted.prime.matches("katu prime").count(),
        1,
        "o prime aparece uma única vez"
    );
    assert_eq!(
        compacted,
        session.context(BUDGET, CompactionMode::Enabled)?,
        "determinístico para o mesmo log"
    );

    // Recuperação obrigatória: uma mensagem do prefixo continua endereçável no log.
    let events = session.log_events()?;
    let all = derive_messages(&events);
    let dropped = all.first().ok_or("há mensagens no log")?;
    let recovered = recover(&events, &message_id(dropped)).ok_or("recuperável")?;
    assert_eq!(&recovered, dropped);
    Ok(())
}

#[test]
fn context_without_overflow_is_plain_even_when_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let mut session = Session::open(&fs, dir)?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "oi".into(),
        visibility: Visibility::User,
    })?;

    let compacted = session.context(BUDGET, CompactionMode::Enabled)?;
    assert!(
        compacted.summary.is_none(),
        "sem prefixo fora do orçamento não há digest (nada silencioso)"
    );
    assert_eq!(compacted.messages.len(), 1);
    Ok(())
}
