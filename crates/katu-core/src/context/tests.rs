use super::{
    COMPACTION_SCHEMA_VERSION, CompactionMode, ContextBudget, PRIME_VERSION, PrimeMode, assemble,
    assemble_with_prime, compact, message_id, prime, prime_for, prime_long, prime_with_catalog,
    recover, tokens_from_bytes,
};
use crate::evidence::EvidenceBasis;
use crate::kernel::{Event, derive_messages};

fn budget(raw_min: usize) -> ContextBudget {
    ContextBudget {
        raw_min,
        summary_max: 0,
    }
}

fn conversation() -> Vec<Event> {
    vec![
        Event::UserMessage {
            text: "primeiro pedido".into(),
        },
        Event::AssistantMessage {
            text: "primeira resposta".into(),
        },
        Event::UserMessage {
            text: "segundo pedido".into(),
        },
    ]
}

#[test]
fn prime_is_stable_and_versioned() {
    assert_eq!(prime(), prime(), "o prime é determinístico");
    assert!(prime().contains(&format!("v{PRIME_VERSION}")));
    assert!(prime().contains("TOON"));
}

#[test]
fn prime_appears_once_in_the_context() {
    let context = assemble(&conversation(), budget(1_000));
    assert_eq!(context.prime, prime());
    assert!(!context.prime.is_empty());
}

#[test]
fn every_message_comes_from_the_log() {
    let events = conversation();
    let context = assemble(&events, budget(1_000));
    assert_eq!(context.messages, derive_messages(&events));
}

#[test]
fn budget_is_respected_and_keeps_the_latest() {
    let events = conversation();
    let full = assemble(&events, budget(1_000));
    assert_eq!(full.messages.len(), 3);
    assert!(full.raw_tokens <= 1_000);
    let tight = assemble(&events, budget(4));
    assert_eq!(tight.messages.len(), 1, "só a mensagem mais recente cabe");
    assert_eq!(
        tight.messages,
        derive_messages(&events)
            .get(2..)
            .unwrap_or_default()
            .to_vec()
    );
}

#[test]
fn exact_limit_keeps_and_limit_minus_one_drops() {
    let events = vec![Event::UserMessage {
        text: "12345678".into(),
    }];
    let weight = tokens_from_bytes(8);
    assert_eq!(assemble(&events, budget(weight)).messages.len(), 1);
    assert_eq!(assemble(&events, budget(weight - 1)).messages.len(), 0);
}

#[test]
fn zero_budget_keeps_no_message() {
    let context = assemble(&conversation(), budget(0));
    assert!(context.messages.is_empty());
    assert_eq!(context.raw_tokens, 0);
    assert!(context.tokens > 0, "o prime continua presente");
}

fn long_conversation() -> Vec<Event> {
    let mut events = Vec::new();
    for turn in 0..6_u32 {
        events.push(Event::UserMessage {
            text: format!("pedido numero {turn} com algum detalhe para pesar"),
        });
        events.push(Event::AssistantMessage {
            text: format!("resposta numero {turn} igualmente longa para pesar"),
        });
    }
    events
}

fn compaction_budget(raw_min: usize, summary_max: usize) -> ContextBudget {
    ContextBudget {
        raw_min,
        summary_max,
    }
}

#[test]
fn compact_is_deterministic() {
    let events = long_conversation();
    let budget = compaction_budget(4, 200);
    let first = compact(&events, budget, CompactionMode::Enabled);
    let second = compact(&events, budget, CompactionMode::Enabled);
    assert!(first.is_some(), "ligada devolve compactação");
    assert_eq!(first, second, "mesmo input → mesma compactação");
}

#[test]
fn disabled_compaction_preserves_assemble() {
    let events = long_conversation();
    let budget = compaction_budget(4, 200);
    assert!(compact(&events, budget, CompactionMode::Disabled).is_none());
    assert_eq!(assemble(&events, budget).summary, None);
}

#[test]
fn compact_keeps_every_original_addressable() -> Result<(), Box<dyn std::error::Error>> {
    let events = long_conversation();
    let budget = compaction_budget(4, 200);
    let compaction = compact(&events, budget, CompactionMode::Enabled)
        .ok_or("compactação devia estar ligada")?;
    assert_eq!(compaction.schema_version, COMPACTION_SCHEMA_VERSION);
    assert!(!compaction.replacements.is_empty());
    for replacement in &compaction.replacements {
        let recovered = recover(&events, &replacement.original)
            .ok_or_else(|| format!("original {} não recuperável", replacement.original))?;
        assert_eq!(message_id(&recovered), replacement.original);
    }
    Ok(())
}

#[test]
fn compaction_reports_inferred_gain() -> Result<(), Box<dyn std::error::Error>> {
    let events = long_conversation();
    let budget = compaction_budget(4, 20);
    let compaction = compact(&events, budget, CompactionMode::Enabled)
        .ok_or("compactação devia estar ligada")?;
    assert_eq!(compaction.gain.basis, EvidenceBasis::Inferred);
    assert_eq!(compaction.gain.name, "context.gain_tokens");
    assert!(
        compaction.original_tokens >= compaction.context.raw_tokens,
        "o ganho compara com o histórico original"
    );
    Ok(())
}

#[test]
fn summary_respects_the_tokens_ceiling() -> Result<(), Box<dyn std::error::Error>> {
    let events = long_conversation();
    let budget = compaction_budget(4, 8);
    let compaction = compact(&events, budget, CompactionMode::Enabled)
        .ok_or("compactação devia estar ligada")?;
    let summary = compaction.context.summary.as_deref().unwrap_or_default();
    assert!(
        tokens_from_bytes(summary.len()) <= 8,
        "o resumo respeita summary_max"
    );
    Ok(())
}

#[test]
fn no_prefix_means_no_replacements() -> Result<(), Box<dyn std::error::Error>> {
    let events = long_conversation();
    let budget = compaction_budget(10_000, 200);
    let compaction = compact(&events, budget, CompactionMode::Enabled)
        .ok_or("compactação devia estar ligada")?;
    assert!(compaction.replacements.is_empty());
    assert_eq!(compaction.context.messages, derive_messages(&events));
    Ok(())
}

#[test]
fn prime_with_catalog_injects_the_tool_table() {
    let catalog = "\u{1e}tool\nread\u{1f}path view?\n";
    let rendered = prime_with_catalog(catalog);
    assert!(rendered.contains("tabela `tool`"), "{rendered}");
    assert!(rendered.contains(catalog), "{rendered}");
    assert!(!rendered.contains("tools: read/write"), "{rendered}");
}

#[test]
fn long_prime_is_versioned_and_longer() {
    let compact = prime();
    let long = prime_long();
    assert!(long.contains(&format!("v{PRIME_VERSION}")));
    assert!(long.len() > compact.len());
    assert_eq!(prime_for(PrimeMode::Long), long);
    assert_eq!(prime_for(PrimeMode::Compact), compact);
}

#[test]
fn assemble_with_prime_uses_the_long_variant() {
    let events = conversation();
    let budget = budget(1_000);
    let context = assemble_with_prime(&events, budget, PrimeMode::Long);
    assert_eq!(context.prime, prime_long());
    assert_eq!(context.messages, derive_messages(&events));
}
