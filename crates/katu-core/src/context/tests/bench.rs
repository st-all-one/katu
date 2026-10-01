//! A/B da seleção por **informação** (Q-02b/Q-03) — determinístico, sem modelo e sem relógio.
//!
//! A pergunta: com o **mesmo** orçamento, a política de utilidade retém mais informação do que a
//! truncagem histórica (sufixo cru / cronologia do digest)?
//!
//! A medida é `I_ret` por token (`I_ret = Σ idf(t)` sobre os termos retidos, `idf(t) = ln(1 + n/df)`),
//! com base `inferred`: é um **proxy** de sucesso de tarefa, não a tarefa. O critério de 20 % do
//! plano aplica-se ao proxy; a adoção do *default* continua a exigir A/B com o modelo.
//!
//! Corre-se com `KATU_SELECT_OUT=bench/e18/select/raw.json cargo test -p katu-core -- --ignored
//! --nocapture ab_context_selection_by_information`; o artefacto é publicado e `bench/published.toml`
//! cita-o (DF5). `check-diag` proíbe `println!` em `crates/`: o número sai para **ficheiro**.

mod rate_distortion;

use rate_distortion::{distortion, intra_similarity, probe_swaps, rate, tokens_total};

use crate::context::{
    AssembleOptions, CompactionMode, ContextBudget, Digest, PrimeMode, SelectionParams,
    SelectionPolicy, Stats, assemble_all, chosen_units, digest, dropped_messages, kept_messages,
    message_text, terms, tokens_from_bytes, units,
};
use crate::error::ToolOutcome;
use crate::evidence::{from_f64, to_f64};
use crate::kernel::{CallId, Event, Message, derive_messages};

/// Orçamento do cru: cabe ~40 % do histórico (é onde a política de seleção decide).
pub(super) const RAW_MIN: usize = 900;

/// Teto do digest.
pub(super) const SUMMARY_MAX: usize = 220;

/// Objetivo do cenário (alimenta o canal `objetivo` da fusão RRF).
pub(super) const GOAL: &str = "reconstruir o pipeline sem perder o que importa";

/// Medição de uma política sob um orçamento.
#[derive(Debug, Clone, Copy)]
pub(super) struct Measurement {
    /// Unidades retidas.
    units: usize,
    /// Tokens retidos.
    tokens: usize,
    /// Mensagens fora do orçamento.
    dropped: usize,
    /// `I_ret` em milésimos de nat.
    information: u64,
    /// `I_ret` por token, em milésimos.
    efficiency: u64,
    /// Distorção `D = 1 − I_ret(S)/I_ret(U)` em milésimos (A1: fração de informação perdida).
    pub(super) distortion: u64,
    /// Similaridade de Jaccard média **intra-conjunto** das unidades escolhidas, em milésimos (A2).
    pub(super) mean_similarity: u64,
    /// Similaridade de Jaccard **máxima** entre duas unidades escolhidas, em milésimos (A2).
    pub(super) max_similarity: u64,
}

/// Termos de um conjunto de mensagens (a informação **retida**).
pub(super) fn retained_terms(messages: &[Message]) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for message in messages {
        out.extend(terms(&message_text(message)));
    }
    out
}

/// `I_ret` por token (0 quando não há tokens).
pub(super) fn efficiency(information: u64, tokens: usize) -> u64 {
    if tokens == 0 {
        return 0;
    }
    information
        .saturating_mul(1_000)
        .checked_div(u64::try_from(tokens).unwrap_or(1))
        .unwrap_or(0)
}

/// Mede uma política sobre um log.
pub(super) fn measure_policy(events: &[Event], policy: SelectionPolicy) -> Measurement {
    let messages = derive_messages(events);
    let all = units(&messages);
    let candidates: Vec<_> = all.iter().map(|unit| unit.candidate.clone()).collect();
    let stats = Stats::of(&candidates);
    let chosen = chosen_units(&all, RAW_MIN, policy, GOAL, SelectionParams::default());
    let kept = kept_messages(&messages, &all, &chosen);
    let tokens: usize = chosen
        .iter()
        .filter_map(|index| all.get(*index))
        .map(|unit| unit.candidate.tokens)
        .sum();
    let information = stats.mass_milli(&retained_terms(&kept));
    let (mean_similarity, max_similarity) = intra_similarity(&all, &chosen);
    Measurement {
        units: chosen.len(),
        tokens,
        dropped: dropped_messages(&messages, &all, &chosen).len(),
        information,
        efficiency: efficiency(information, tokens),
        distortion: distortion(stats.mass_milli(&retained_terms(&messages)), information),
        mean_similarity,
        max_similarity,
    }
}

/// Um `ToolUse` de leitura (para as unidades de corrida do cenário).
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

/// Cenário determinístico: enchimento repetitivo + marcos informativos + lotes de tool.
///
/// O que informa (termos que só aparecem uma vez) está **espalhado**, incluindo o fim do prefixo —
/// exatamente o que a truncagem histórica corta.
pub(super) fn scenario() -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    let use_ = read_use()?;
    let milestones = [
        "o zircao cristaliza a 1855 graus",
        "o indice rrf usa k igual sessenta",
        "a divergencia js mede a cobertura",
        "o orcamento cru reserva mil tokens",
    ];
    let mut events = vec![Event::UserMessage {
        text: GOAL.to_string(),
    }];
    for turn in 0..40_u32 {
        events.push(Event::AssistantMessage {
            text: format!("katu passo {turn} katu passo repetido katu"),
        });
        events.push(Event::UserMessage {
            text: format!("pedido {turn} katu repetido katu repetido"),
        });
        if turn % 10 == 3 {
            let index = usize::try_from(turn / 10).unwrap_or(0);
            if let Some(text) = milestones.get(index) {
                events.push(Event::AssistantMessage {
                    text: (*text).to_string(),
                });
            }
        }
        if turn % 10 == 7 {
            push_tool_batch(&mut events, turn, &use_);
        }
    }
    Ok(events)
}

/// Um lote B-01 (N pedidos seguidos de N resultados) — exercita a unidade de corrida.
fn push_tool_batch(events: &mut Vec<Event>, turn: u32, use_: &katu_policy::ToolUse) {
    for call in ["c1", "c2"] {
        events.push(Event::ToolCall {
            call: CallId::new(format!("{turn}-{call}")),
            tool: use_.clone(),
        });
    }
    for call in ["c1", "c2"] {
        events.push(Event::ToolResult {
            call: CallId::new(format!("{turn}-{call}")),
            outcome: ToolOutcome::Ok,
            delta: Some(format!(
                "r\nread.summary f_{turn}_{call}\nk\npath src/main.rs"
            )),
        });
    }
}

/// Cenário **uniforme** (controlo negativo): cada mensagem traz termos próprios, sem redundância.
///
/// Aqui a utilidade não tem de onde tirar vantagem; um ganho grande significaria que a métrica mede
/// outra coisa.
fn uniform_scenario() -> Vec<Event> {
    let mut events = vec![Event::UserMessage {
        text: GOAL.to_string(),
    }];
    for turn in 0..40_u32 {
        events.push(Event::AssistantMessage {
            text: format!("termo{turn} alfa{turn} beta{turn} gama{turn}"),
        });
        events.push(Event::UserMessage {
            text: format!("pedido{turn} delta{turn} epsilon{turn} zeta{turn}"),
        });
    }
    events
}

/// Os dois digests do mesmo prefixo (Q-03): cronológico e por utilidade, com o prefixo contado.
struct DigestPair {
    /// Digest cronológico (truncagem histórica).
    chronological: Digest,
    /// Digest por utilidade (Q-03).
    by_utility: Digest,
    /// Mensagens do prefixo.
    prefix: usize,
}

fn digest_pair(events: &[Event]) -> Result<DigestPair, Box<dyn std::error::Error>> {
    let messages = derive_messages(events);
    let all = units(&messages);
    let chosen = chosen_units(
        &all,
        RAW_MIN,
        SelectionPolicy::Utility,
        GOAL,
        SelectionParams::default(),
    );
    let kept = kept_messages(&messages, &all, &chosen);
    let prefix = dropped_messages(&messages, &all, &chosen);
    let params = SelectionParams::default();
    let chronological = digest(&prefix, &kept, SUMMARY_MAX, SelectionPolicy::Suffix, params)?;
    let by_utility = digest(
        &prefix,
        &kept,
        SUMMARY_MAX,
        SelectionPolicy::Utility,
        params,
    )?;
    Ok(DigestPair {
        chronological,
        by_utility,
        prefix: prefix.len(),
    })
}

/// Tokens estimados de um texto (o mesmo rácio do orçamento).
fn tokens_of(text: &str) -> u64 {
    u64::try_from(tokens_from_bytes(text.len())).unwrap_or(u64::MAX)
}

/// Ganho percentual de `current` sobre `baseline` (0 se a base for nula).
fn percent(current: u64, baseline: u64) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    ((to_f64(current) - to_f64(baseline)) / to_f64(baseline) * 10_000.0).round() / 100.0
}

/// `I_ret` por token de um digest, em milésimos.
fn digest_efficiency(digest: &Digest) -> u64 {
    let tokens = tokens_of(&digest.summary);
    if tokens == 0 {
        return 0;
    }
    from_f64(digest.information.value * 1_000.0 / to_f64(tokens))
}

/// A/B completo (Q-02b + Q-03), devolvendo o JSON do artefacto.
#[allow(
    clippy::too_many_lines,
    reason = "é o artefacto: cada campo é um número publicado; partir em helpers esconderia a conta"
)]
fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let events = scenario()?;
    let suffix = measure_policy(&events, SelectionPolicy::Suffix);
    let utility = measure_policy(&events, SelectionPolicy::Utility);
    let control_suffix = measure_policy(&uniform_scenario(), SelectionPolicy::Suffix);
    let control_utility = measure_policy(&uniform_scenario(), SelectionPolicy::Utility);
    let probe = probe_swaps(&events);
    let pair = digest_pair(&events)?;
    let (chronological, by_utility) = (&pair.chronological, &pair.by_utility);
    let prefix_len = pair.prefix;
    let selection_gain = percent(utility.efficiency, suffix.efficiency);
    let digest_gain = percent(
        digest_efficiency(by_utility),
        digest_efficiency(chronological),
    );
    let budget = ContextBudget {
        raw_min: RAW_MIN,
        summary_max: SUMMARY_MAX,
    };
    let assembly = assemble_all(
        &events,
        budget,
        AssembleOptions {
            selection: SelectionPolicy::Utility,
            goal: GOAL,
            ..AssembleOptions::new(PrimeMode::Compact, CompactionMode::Enabled)
        },
    );
    let value = serde_json::json!({
        "schema": "katu.bench.select.v1",
        "question": "context_by_information",
        "messages": derive_messages(&events).len(),
        "units": units(&derive_messages(&events)).len(),
        "raw_min": RAW_MIN,
        "summary_max": SUMMARY_MAX,
        "goal": GOAL,
        "suffix": {
            "units": suffix.units,
            "tokens": suffix.tokens,
            "dropped_messages": suffix.dropped,
            "information_milli": suffix.information,
            "efficiency_milli_per_token": suffix.efficiency,
            "distortion_milli": suffix.distortion,
            "mean_similarity_milli": suffix.mean_similarity,
            "max_similarity_milli": suffix.max_similarity,
        },
        "utility": {
            "units": utility.units,
            "tokens": utility.tokens,
            "dropped_messages": utility.dropped,
            "information_milli": utility.information,
            "efficiency_milli_per_token": utility.efficiency,
            "distortion_milli": utility.distortion,
            "mean_similarity_milli": utility.mean_similarity,
            "max_similarity_milli": utility.max_similarity,
        },
        "a1_rate_distortion": {
            "definition": "R = tokens(S)/tokens(U); D = 1 - I_ret(S)/I_ret(U); D0 = distorcao do historico (sufixo) na mesma taxa",
            "rate_milli_suffix": rate(suffix.tokens, tokens_total(&events)),
            "rate_milli_utility": rate(utility.tokens, tokens_total(&events)),
            "d0_milli": suffix.distortion,
            "d_milli": utility.distortion,
            "criterion_met": utility.distortion <= suffix.distortion,
        },
        "a2_diversity": {
            "definition": "Jaccard intra-conjunto das unidades escolhidas (media e maximo); sem DPP",
            "sim_max_milli": SelectionParams::default().sim_max_milli,
            "max_utility_milli": utility.max_similarity,
            "max_suffix_milli": suffix.max_similarity,
            "mean_utility_milli": utility.mean_similarity,
            "mean_suffix_milli": suffix.mean_similarity,
            "criterion_met": utility.max_similarity
                <= u64::from(SelectionParams::default().sim_max_milli),
        },
        "a2_dpp_decision": {
            "definition": "DPP (determinantal) maximiza log-det; a 1a ordem e' penalizar redundancia",
            "best_swap_gain_milli": probe.gain_milli,
            "improving_swaps": probe.improving,
            "decision": "rejeitado: o MMR ja resolve a diversidade (media 17 per-mil, maximo 500 per-mil <= sim_max 700 per-mil; o historico chega a 1000 per-mil). As trocas que sobram sao de UTILIDADE, nao de redundancia (nao ha par redundante para o determinante penalizar) - o que as fecharia e uma busca local do greedy, nao um DPP. Reavaliar se, com log real, a similaridade media passar de sim_max/2",
        },
        "selection_gain_pct": selection_gain,
        "control_uniform": {
            "suffix_efficiency_milli_per_token": control_suffix.efficiency,
            "utility_efficiency_milli_per_token": control_utility.efficiency,
            "gain_pct": percent(control_utility.efficiency, control_suffix.efficiency),
        },
        "digest": {
            "prefix_messages": prefix_len,
            "chronological": {
                "rows": chronological.kept_rows,
                "tokens": tokens_of(&chronological.summary),
                "information_milli": chronological.information.value,
                "js_milli": chronological.divergence.value,
            },
            "utility": {
                "rows": by_utility.kept_rows,
                "tokens": tokens_of(&by_utility.summary),
                "information_milli": by_utility.information.value,
                "js_milli": by_utility.divergence.value,
            },
            "information_gain_pct": digest_gain,
        },
        "compact_applied": assembly.compaction.is_some(),
        "criterion_pct": 20.0,
        "criterion_met_on_proxy": selection_gain >= 20.0 && digest_gain >= 20.0,
        "caveat": "I_ret (idf) é proxy de informação, não sucesso de tarefa: adotar o default exige A/B com o modelo",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// A/B publicado: escreve o artefacto em `KATU_SELECT_OUT` (opt-in) e afirma o invariante do plano.
#[test]
#[ignore = "A/B de Q-02b/Q-03: escreve o artefacto em KATU_SELECT_OUT (não é asserção de CI)"]
fn ab_context_selection_by_information() -> Result<(), Box<dyn std::error::Error>> {
    let json = measure()?;
    if let Some(path) = std::env::var_os("KATU_SELECT_OUT") {
        std::fs::write(path, format!("{json}\n"))?;
    }
    let value: serde_json::Value = serde_json::from_str(&json)?;
    let efficiency = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.get("efficiency_milli_per_token"))
            .and_then(serde_json::Value::as_u64)
    };
    let suffix = efficiency("suffix").ok_or("sem eficiência do sufixo")?;
    let utility = efficiency("utility").ok_or("sem eficiência da utilidade")?;
    assert!(
        utility >= suffix,
        "a utilidade nunca pode reter menos informação por token: {utility} < {suffix}"
    );
    Ok(())
}
