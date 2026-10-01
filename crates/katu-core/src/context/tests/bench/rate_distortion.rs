//! Taxa–distorção (A1) e diversidade (A2) da seleção por informação.
//!
//! O A/B de Q-02b mede **eficiência** (`I_ret` por token), que é um proxy sem contrato. A1
//! formaliza-o como uma curva taxa–distorção com um contrato comparável à política histórica;
//! A2 mede a diversidade pelo critério de aceitação do Anexo A (nenhum par escolhido acima de
//! `sim_max`) e decide o DPP **com o número**, não por Euforia.
//!
//! Tudo aqui é determinístico (sem relógio, sem RNG) e só corre no A/B.

use crate::context::select::Unit;
use crate::context::{
    SelectionParams, SelectionPolicy, Stats, chosen_units, jaccard_milli, kept_messages,
    message_text, tokens_from_bytes, units,
};
use crate::kernel::{Event, derive_messages};

use super::{GOAL, RAW_MIN, retained_terms};

/// Resultado da sondagem de trocas 1‑para‑1 (a decisão sobre o DPP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SwapProbe {
    /// Ganho de massa da melhor troca que cabe no orçamento, em milésimos.
    pub(super) gain_milli: u64,
    /// Nº de trocas que aumentam a informação.
    pub(super) improving: usize,
}

/// Distorção `D = 1 − I_ret(S)/I_ret(U)` em milésimos (A1: fração de informação perdida).
///
/// `U` é o histórico **inteiro** (a referência de qualidade) e `S` o conjunto retido. `D = 0`
/// significa que nada de informação se perdeu; `D = 1 000` que nada sobreviveu.
pub(super) fn distortion(full_milli: u64, kept_milli: u64) -> u64 {
    let _span = crate::trace_fn!("context::bench::distortion");

    if full_milli == 0 {
        return 0;
    }
    full_milli
        .saturating_sub(kept_milli)
        .saturating_mul(1_000)
        .checked_div(full_milli)
        .unwrap_or(0)
}

/// Similaridade intra-conjunto (média e máximo) das unidades escolhidas, em milésimos (A2).
pub(super) fn intra_similarity(all: &[Unit], chosen: &[usize]) -> (u64, u64) {
    let _span = crate::trace_fn!("context::bench::intra_similarity");

    let mut sum = 0_u64;
    let mut pairs = 0_u64;
    let mut max = 0_u64;
    for (position, &left) in chosen.iter().enumerate() {
        for &right in chosen.iter().skip(position.saturating_add(1)) {
            let (Some(left), Some(right)) = (all.get(left), all.get(right)) else {
                continue;
            };
            let js = jaccard_milli(&left.candidate.terms, &right.candidate.terms);
            sum = sum.saturating_add(js);
            pairs = pairs.saturating_add(1);
            max = max.max(js);
        }
    }
    if pairs == 0 {
        return (0, 0);
    }
    (sum.checked_div(pairs).unwrap_or(0), max)
}

/// Tokens do histórico inteiro (a taxa `R` de A1).
pub(super) fn tokens_total(events: &[Event]) -> usize {
    let _span = crate::trace_fn!("context::bench::tokens_total");

    derive_messages(events)
        .iter()
        .map(|message| tokens_from_bytes(message_text(message).len()))
        .sum()
}

/// Taxa `R = tokens(S)/tokens(U)` em milésimos (A1).
pub(super) fn rate(kept: usize, full: usize) -> u64 {
    let _span = crate::trace_fn!("context::bench::rate");

    if full == 0 {
        return 0;
    }
    u64::try_from(kept)
        .unwrap_or(u64::MAX)
        .saturating_mul(1_000)
        .checked_div(u64::try_from(full).unwrap_or(1))
        .unwrap_or(0)
}

/// Sonda as trocas 1‑para‑1 que cabem no orçamento (a medição que decide o DPP).
///
/// O DPP maximiza `log det`; à primeira ordem isso é penalizar **redundância**. A sonda pergunta o
/// que há a ganhar com uma troca simples: se as trocas que melhoram a informação são de
/// *utilidade* e nao de *redundancia*, o determinante não tem o que maximizar. Determinístico:
/// varre escolhidas × não-escolhidas pela ordem do índice.
pub(super) fn probe_swaps(events: &[Event]) -> SwapProbe {
    let _span = crate::trace_fn!("context::bench::probe_swaps");

    let messages = derive_messages(events);
    let all = units(&messages);
    let candidates: Vec<_> = all.iter().map(|unit| unit.candidate.clone()).collect();
    let stats = Stats::of(&candidates);
    let chosen = chosen_units(
        &all,
        RAW_MIN,
        SelectionPolicy::Utility,
        GOAL,
        SelectionParams::default(),
    );
    let base_tokens: usize = chosen
        .iter()
        .filter_map(|index| all.get(*index))
        .map(|unit| unit.candidate.tokens)
        .sum();
    let mut best = 0_u64;
    let mut improving = 0_usize;
    for (position, &out) in chosen.iter().enumerate() {
        let Some(left) = all.get(out) else { continue };
        let without_tokens = base_tokens.saturating_sub(left.candidate.tokens);
        let kept_without: Vec<usize> = chosen
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != position)
            .map(|(_, unit)| *unit)
            .collect();
        let base_without = stats.mass_milli(&retained_terms(&kept_messages(
            &messages,
            &all,
            &kept_without,
        )));
        for (incoming, unit) in all.iter().enumerate() {
            if chosen.contains(&incoming) {
                continue;
            }
            let cost = without_tokens.saturating_add(unit.candidate.tokens);
            let (Some(incoming_message), true) = (messages.get(unit.start), cost <= RAW_MIN) else {
                continue;
            };
            let mut swapped = kept_messages(&messages, &all, &kept_without);
            swapped.push(incoming_message.clone());
            let gain = stats
                .mass_milli(&retained_terms(&swapped))
                .saturating_sub(base_without);
            best = best.max(gain);
            if gain > 0 {
                improving = improving.saturating_add(1);
            }
        }
    }
    SwapProbe {
        gain_milli: best,
        improving,
    }
}
