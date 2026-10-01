//! Testes de S-01/Q-02b/Q-03/Q-04: um só caminho de orçamento, seleção por informação e a secção
//! de estado no prime.

use crate::context::{
    AssembleOptions, CompactionMode, ContextBudget, MAX_SECTION_BYTES, PrimeMode, SelectionParams,
    SelectionPolicy, StateView, Stats, assemble_all, chosen_units, digest, kept_messages, prime,
    state_section, tokens_from_bytes, units,
};
use crate::kernel::{Event, derive_messages};

use super::bench::{GOAL, RAW_MIN, SUMMARY_MAX, efficiency, retained_terms, scenario};
use super::{budget, compaction_budget, conversation, long_conversation};

/// S-01: uma só passagem pelo log devolve contexto **e** compactação, e o prefixo é exatamente o
/// que não coube no orçamento cru.
#[test]
fn assemble_all_returns_the_context_and_the_compaction_in_one_pass()
-> Result<(), Box<dyn std::error::Error>> {
    let events = long_conversation();
    let budget = compaction_budget(20, 200);
    let assembly = assemble_all(
        &events,
        budget,
        AssembleOptions::new(PrimeMode::Compact, CompactionMode::Enabled),
    );
    let compaction = assembly.compaction.ok_or("devia compactar")?;
    assert_eq!(assembly.dropped, compaction.replacements.len());
    assert_eq!(compaction.context.messages, assembly.context.messages);
    assert_eq!(compaction.context.summary, assembly.context.summary);
    Ok(())
}

/// Q-03: com a política de utilidade, o digest retém **mais informação** do que a truncagem
/// cronológica com o mesmo teto — o critério que o A/B publica (`bench/e18/select/`).
#[test]
fn a_utility_digest_retains_more_information_than_the_chronological_one()
-> Result<(), Box<dyn std::error::Error>> {
    // Prefixo: as duas primeiras mensagens falam do termo raro; o resto é repetição.
    let mut events = Vec::new();
    for index in 0..12 {
        events.push(Event::UserMessage {
            text: format!("katu passo {index} katu passo"),
        });
    }
    // O que informa está no **fim** do prefixo — e é exatamente o que a truncagem cronológica corta.
    events.push(Event::UserMessage {
        text: "investiga o zircao e o seu ciclo".into(),
    });
    events.push(Event::AssistantMessage {
        text: "o zircao cristaliza a 1855 graus".into(),
    });
    let budget = compaction_budget(10, 40);
    let chronological = digest(
        &derive_messages(&events),
        &[],
        budget.summary_max,
        SelectionPolicy::Suffix,
        SelectionParams::default(),
    )?;
    let utility = digest(
        &derive_messages(&events),
        &[],
        budget.summary_max,
        SelectionPolicy::Utility,
        SelectionParams::default(),
    )?;
    assert!(
        utility.information.value >= chronological.information.value,
        "I_ret utilidade {} < cronológico {}",
        utility.information.value,
        chronological.information.value
    );
    assert!(
        utility.information.value > chronological.information.value,
        "o digest por utilidade tem de reter mais: {} vs {}",
        utility.information.value,
        chronological.information.value
    );
    assert!(
        utility.summary.contains("zircao"),
        "o digest por utilidade não perde o termo raro: {}",
        utility.summary
    );
    Ok(())
}

/// Q-03: um prefixo que o sufixo já cobre não se paga — o gatilho `τ_JS` recusa a compactação.
#[test]
fn the_utility_digest_is_skipped_when_the_prefix_is_redundant() {
    let mut events = Vec::new();
    for index in 0..8 {
        events.push(Event::UserMessage {
            text: format!("katu passo {index}"),
        });
    }
    let budget = compaction_budget(2, 200);
    let assembly = assemble_all(
        &events,
        budget,
        AssembleOptions {
            selection: SelectionPolicy::Utility,
            ..AssembleOptions::new(PrimeMode::Compact, CompactionMode::Enabled)
        },
    );
    assert!(
        assembly.dropped > 0,
        "há prefixo fora do orçamento neste cenário"
    );
    assert!(
        assembly.compaction.is_none(),
        "prefixo redundante não devia gastar o orçamento do resumo"
    );
    assert!(assembly.context.summary.is_none());
}

/// Q-04: a secção de estado é determinística, curta e diz os factos que o modelo precisa.
#[test]
fn the_state_section_is_deterministic_short_and_informative() {
    let rules = vec![
        "contain-sensitive-read".to_string(),
        "plan-write".to_string(),
    ];
    let working = vec!["src/main.rs".to_string(), "src/main.rs".to_string()];
    let view = StateView {
        mode: "plano",
        rules: &rules,
        steps_max: Some(6),
        compaction: true,
        working_set: &working,
    };
    let section = state_section(&view);
    assert_eq!(section, state_section(&view), "determinística");
    assert!(section.len() <= MAX_SECTION_BYTES);
    assert!(section.starts_with("estado:\n"), "{section}");
    assert!(section.contains("modo plano"), "{section}");
    assert!(
        section.contains("regras contain-sensitive-read,plan-write"),
        "{section}"
    );
    assert!(section.contains("passos 6"), "{section}");
    assert!(section.contains("compactacao ligada"), "{section}");
    assert!(
        section.contains("tocados src/main.rs"),
        "sem duplicados: {section}"
    );
    assert_eq!(
        section.matches("src/main.rs").count(),
        1,
        "o working set não repete caminhos"
    );
}

/// Q-04: sem factos opcionais, a secção continua legível (`-` e `?`), nunca vazia.
#[test]
fn an_empty_state_is_still_renderable() {
    let view = StateView {
        mode: "execucao",
        rules: &[],
        steps_max: None,
        compaction: false,
        working_set: &[],
    };
    let section = state_section(&view);
    assert!(section.contains("regras -"), "{section}");
    assert!(section.contains("passos ?"), "{section}");
    assert!(section.contains("tocados -"), "{section}");
    assert!(section.contains("compactacao desligada"), "{section}");
}

/// Q-04: a secção entra **no fim** do prime (o resto do prompt continua prefixo estável).
#[test]
fn the_state_section_is_appended_to_the_prime() {
    let events = conversation();
    let view = StateView {
        mode: "execucao",
        rules: &[],
        steps_max: Some(4),
        compaction: false,
        working_set: &[],
    };
    let section = state_section(&view);
    let context = assemble_all(
        &events,
        budget(1_000),
        AssembleOptions {
            state: Some(&section),
            ..AssembleOptions::default()
        },
    )
    .context;
    assert!(context.prime.starts_with(&prime()), "{:?}", context.prime);
    assert!(context.prime.ends_with(&section), "{:?}", context.prime);
}

/// O invariante do plano como teste de CI: `U(utilidade) ≥ U(sufixo)` e orçamento exato, em todos os
/// orçamentos — sem wall-clock nem artefacto (por isso **não** é `#[ignore]`).
#[test]
fn the_utility_selection_never_loses_to_the_suffix_at_equal_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let events = scenario()?;
    let messages = derive_messages(&events);
    let all = units(&messages);
    let candidates: Vec<_> = all.iter().map(|unit| unit.candidate.clone()).collect();
    let stats = Stats::of(&candidates);
    let mut suffix_best = 0_u64;
    let mut utility_best = 0_u64;
    for raw_min in [0, 40, 120, 400, 900, 4_000] {
        for policy in [SelectionPolicy::Suffix, SelectionPolicy::Utility] {
            let chosen = chosen_units(&all, raw_min, policy, GOAL, SelectionParams::default());
            let kept = kept_messages(&messages, &all, &chosen);
            let tokens: usize = chosen
                .iter()
                .filter_map(|index| all.get(*index))
                .map(|unit| unit.candidate.tokens)
                .sum();
            assert!(tokens <= raw_min, "orçamento {raw_min} excedido: {tokens}");
            let measured = efficiency(stats.mass_milli(&retained_terms(&kept)), tokens);
            match policy {
                SelectionPolicy::Suffix => suffix_best = suffix_best.max(measured),
                SelectionPolicy::Utility => utility_best = utility_best.max(measured),
            }
        }
    }
    assert!(
        utility_best >= suffix_best,
        "em nenhum orçamento a utilidade foi melhor: {utility_best} < {suffix_best}"
    );
    Ok(())
}

/// A secção de estado (Q-04) entra no prime **no fim** e o orçamento continua exato (S-01).
#[test]
fn the_state_section_keeps_the_budget_exact() -> Result<(), Box<dyn std::error::Error>> {
    let events = scenario()?;
    let budget = ContextBudget {
        raw_min: RAW_MIN,
        summary_max: SUMMARY_MAX,
    };
    let rules = vec!["contain-sensitive-read".to_string()];
    let view = StateView {
        mode: "plano",
        rules: &rules,
        steps_max: Some(8),
        compaction: true,
        working_set: &[],
    };
    let section = state_section(&view);
    let with = assemble_all(
        &events,
        budget,
        AssembleOptions {
            state: Some(&section),
            ..AssembleOptions::default()
        },
    );
    let without = assemble_all(&events, budget, AssembleOptions::default());
    assert_eq!(
        with.context.messages, without.context.messages,
        "o estado não muda a seleção das mensagens"
    );
    assert!(with.context.prime.ends_with(&section));
    assert_eq!(
        with.context.tokens,
        without.context.tokens + tokens_from_bytes(section.len()),
        "o estado conta para o orçamento (nada entra sem contabilidade)"
    );
    Ok(())
}
