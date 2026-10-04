//! Testes da seleção por informação (Q-02b/Q-03).

use crate::kernel::Visibility;
use std::collections::BTreeSet;

use super::{
    Candidate, SELECTION_SCHEMA_VERSION, SelectionParams, SelectionPolicy, Stats, chosen_units,
    dropped_messages, greedy, jaccard_milli, js_milli, kept_messages, message_text, suffix_start,
    term_counts, terms, units,
};
use crate::error::ToolOutcome;
use crate::kernel::{CallId, Event, Message, derive_messages};

/// Um `ToolUse` de leitura mínimo, para construir mensagens de tool nos testes.
fn read_use() -> Result<katu_policy::ToolUse, katu_policy::PolicyError> {
    let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
    Ok(katu_policy::ToolUse {
        name: katu_policy::ToolName::Read,
        args: katu_policy::ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

/// Conversa simples: cada mensagem é uma unidade.
fn conversation() -> Vec<Message> {
    derive_messages(&[
        Event::UserMessage {
            text: "primeiro pedido".into(),
            visibility: Visibility::User,
        },
        Event::AssistantMessage {
            text: "primeira resposta".into(),
        },
        Event::UserMessage {
            text: "segundo pedido".into(),
            visibility: Visibility::User,
        },
    ])
}

#[test]
fn the_schema_is_versioned() {
    assert_eq!(SELECTION_SCHEMA_VERSION, 1);
    assert_eq!(SelectionParams::DEFAULT, SelectionParams::default());
}

#[test]
fn terms_are_lowercased_and_short_tokens_are_noise() {
    let got = terms("Lê o Ficheiro katu.rs e o a de");
    assert!(got.contains("ficheiro"), "{got:?}");
    assert!(got.contains("katu"), "{got:?}");
    assert!(!got.contains("de"), "termos curtos são ruído");
    assert!(!got.contains("o"));
}

#[test]
fn a_tool_batch_is_one_unit() -> Result<(), Box<dyn std::error::Error>> {
    let use_ = read_use()?;
    let mut events = vec![Event::UserMessage {
        text: "objetivo".into(),
        visibility: Visibility::User,
    }];
    for call in ["c1", "c2"] {
        events.push(Event::ToolCall {
            call: CallId::new(call),
            tool: use_.clone(),
        });
    }
    for call in ["c1", "c2"] {
        events.push(Event::ToolResult {
            call: CallId::new(call),
            outcome: ToolOutcome::Ok,
            delta: Some(format!("conteudo {call}")),
        });
    }
    let messages = derive_messages(&events);
    let units = units(&messages);
    assert_eq!(units.len(), 2, "o lote de tool é uma só unidade: {units:?}");
    let batch = units.get(1).ok_or("o lote é a segunda unidade")?;
    assert_eq!(batch.start, 1);
    assert_eq!(batch.end, 5);
    Ok(())
}

#[test]
fn the_suffix_policy_reproduces_the_historical_cut() {
    let messages = conversation();
    let units = units(&messages);
    // Varre orçamentos: o sufixo é sempre contíguo e nunca excede o teto.
    for budget in 0..=80 {
        let start = suffix_start(&units, budget);
        let kept = kept_messages(&messages, &units, &(start..units.len()).collect::<Vec<_>>());
        assert_eq!(kept.len(), messages.len() - start);
        let used: usize = units
            .get(start..)
            .unwrap_or_default()
            .iter()
            .map(|unit| unit.candidate.tokens)
            .sum();
        assert!(used <= budget, "orçamento {budget} excedido: {used}");
    }
    // Orçamento zero não deixa entrar nada com peso.
    assert_eq!(suffix_start(&units, 0), units.len());
}

#[test]
fn idf_gives_more_mass_to_a_rare_term() {
    let candidates = vec![
        Candidate::of("katu katu katu", 0),
        Candidate::of("katu", 0),
        Candidate::of("katu", 0),
        Candidate::of("sobremesa", 0),
    ];
    let stats = Stats::of(&candidates);
    assert!(
        stats.idf_milli("sobremesa") > stats.idf_milli("katu"),
        "termo raro pesa mais"
    );
    let rare = terms("sobremesa");
    let common = terms("katu");
    assert!(stats.mass_milli(&rare) > stats.mass_milli(&common));
}

#[test]
fn marginal_mass_only_counts_uncovered_terms() {
    let candidates = vec![Candidate::of("katu alfa", 0), Candidate::of("katu beta", 0)];
    let stats = Stats::of(&candidates);
    let covered = terms("katu");
    let marginal = stats.marginal_milli(&terms("katu beta"), &covered);
    assert_eq!(marginal, stats.idf_milli("beta"));
}

#[test]
fn the_utility_policy_keeps_information_that_the_suffix_drops() {
    // O prefixo é o único que fala de `zircao` (termo raro); o sufixo é todo sobre `katu`.
    let mut messages = vec![Message::User {
        text: "investiga o zircao".into(),
        visibility: Visibility::User,
    }];
    for index in 0..6 {
        messages.push(Message::Assistant {
            text: format!("katu passo {index} katu"),
        });
    }
    let units = units(&messages);
    let goal = "zircao";
    let suffix = chosen_units(
        &units,
        30,
        SelectionPolicy::Suffix,
        goal,
        SelectionParams::DEFAULT,
    );
    let utility = chosen_units(
        &units,
        30,
        SelectionPolicy::Utility,
        goal,
        SelectionParams::DEFAULT,
    );
    assert!(!suffix.contains(&0), "o sufixo não chega ao prefixo");
    assert!(
        utility.contains(&0),
        "a utilidade mantém o que informa: {utility:?}"
    );
    assert!(
        utility.contains(&(units.len() - 1)),
        "o turno corrente fica intacto"
    );
}

#[test]
fn the_utility_policy_is_deterministic_and_within_budget() {
    let messages = conversation();
    let units = units(&messages);
    let goal = "pedido";
    let first = chosen_units(
        &units,
        40,
        SelectionPolicy::Utility,
        goal,
        SelectionParams::DEFAULT,
    );
    let second = chosen_units(
        &units,
        40,
        SelectionPolicy::Utility,
        goal,
        SelectionParams::DEFAULT,
    );
    assert_eq!(first, second, "mesmo input, mesma escolha");
    let used: usize = first
        .iter()
        .filter_map(|index| units.get(*index))
        .map(|unit| unit.candidate.tokens)
        .sum();
    assert!(used <= 40, "orçamento excedido: {used}");
}

#[test]
fn mmr_rejects_a_near_duplicate() {
    // `sim_max_milli` a zero rejeita qualquer sobreposição: dois candidatos quase iguais nunca
    // entram juntos.
    let candidates = vec![
        Candidate::of("katu alfa beta gama", 0),
        Candidate::of("katu alfa beta gama", 0),
        Candidate::of("delta epsilon zeta", 0),
    ];
    let params = SelectionParams {
        sim_max_milli: 500,
        ..SelectionParams::DEFAULT
    };
    let chosen = greedy(&candidates, 1_000, None, &BTreeSet::new(), params);
    assert!(chosen.contains(&0) || chosen.contains(&1));
    assert!(!(chosen.contains(&0) && chosen.contains(&1)), "{chosen:?}");
}

#[test]
fn jaccard_is_zero_for_disjoint_and_full_for_equal() {
    assert_eq!(jaccard_milli(&terms("alfa beta"), &terms("gama delta")), 0);
    assert_eq!(
        jaccard_milli(&terms("alfa beta"), &terms("alfa beta")),
        1_000
    );
    assert_eq!(jaccard_milli(&BTreeSet::new(), &BTreeSet::new()), 0);
}

#[test]
fn js_is_zero_for_equal_distributions_and_grows_with_divergence() {
    let left = term_counts([terms("katu alfa beta")]);
    let right = term_counts([terms("katu alfa beta")]);
    assert_eq!(js_milli(&left, &right), 0);
    let other = term_counts([terms("zircao omicron tau")]);
    assert!(
        js_milli(&left, &other) > 0,
        "distribuições disjuntas divergem"
    );
    assert_eq!(js_milli(&term_counts([]), &term_counts([])), 0);
}

#[test]
fn message_text_borrows_when_it_can() -> Result<(), Box<dyn std::error::Error>> {
    let delta = Message::ToolResult {
        call: CallId::new("c1"),
        outcome: ToolOutcome::Ok,
        delta: Some("payload grande".into()),
        tool_name: None,
    };
    assert!(matches!(
        message_text(&delta),
        std::borrow::Cow::Borrowed(_)
    ));
    let assistant = Message::Assistant {
        text: "texto".into(),
    };
    assert!(matches!(
        message_text(&assistant),
        std::borrow::Cow::Borrowed(_)
    ));
    let call = Message::ToolCall {
        call: CallId::new("c1"),
        tool: read_use()?,
    };
    assert!(matches!(message_text(&call), std::borrow::Cow::Owned(_)));
    Ok(())
}

#[test]
fn tool_result_summary_includes_the_tool_name() {
    let denied = Message::ToolResult {
        call: CallId::new("c1"),
        outcome: ToolOutcome::Denied {
            rule_id: katu_policy::RuleId::from("contain-read-outside-workspace"),
            evidence: katu_policy::Evidence::new(
                "fora da raiz",
                "/etc/passwd",
                katu_policy::RuleId::from("contain-read-outside-workspace"),
            )
            .with_remedy(Some("leia só sob a raiz do workspace".to_string())),
        },
        delta: None,
        tool_name: Some(katu_policy::ToolName::Read),
    };
    let text = message_text(&denied);
    assert!(text.starts_with("read: "), "{text}");
    assert!(
        text.contains("fix: leia só sob a raiz do workspace"),
        "{text}"
    );
}

#[test]
fn tool_result_without_tool_name_falls_back_to_the_summary() {
    let denied = Message::ToolResult {
        call: CallId::new("c1"),
        outcome: ToolOutcome::Denied {
            rule_id: katu_policy::RuleId::from("r"),
            evidence: katu_policy::Evidence::new(
                "facto",
                "argumento",
                katu_policy::RuleId::from("r"),
            ),
        },
        delta: None,
        tool_name: None,
    };
    assert_eq!(message_text(&denied), "denied r argumento");
}

#[test]
fn dropped_and_kept_partition_the_history() -> Result<(), Box<dyn std::error::Error>> {
    let messages = conversation();
    let units = units(&messages);
    let chosen = vec![1_usize];
    let kept = kept_messages(&messages, &units, &chosen);
    let dropped = dropped_messages(&messages, &units, &chosen);
    assert_eq!(kept.len() + dropped.len(), messages.len());
    assert_eq!(
        kept,
        vec![messages.get(1).cloned().ok_or("segunda mensagem")?]
    );
    assert_eq!(dropped.len(), 2);
    Ok(())
}
