//! Testes do controlo de múltiplas comparações (C5): p-value exacto, Benjamini–Hochberg e o
//! efeito fail-closed de `control_fdr`.

use super::{
    Confidence, P_FLOOR_MICRO, P_MICRO_SCALE, Threshold, Trials, Verdict, benjamini_hochberg,
    control_fdr,
};
use crate::confidence::verdict;
use crate::rule::RuleId;

/// Acumulador com `successes` de `total` ensaios.
fn trials(successes: u32, total: u32) -> Trials {
    let mut accumulator = Trials::new();
    for index in 0..total {
        if index < successes {
            accumulator.observe_honored();
        } else {
            accumulator.observe_violation();
        }
    }
    accumulator
}

#[test]
fn without_observations_the_p_value_is_one() {
    assert_eq!(Trials::new().promotion_p_micro(900), P_MICRO_SCALE);
}

#[test]
fn the_p_value_is_the_exact_binomial_upper_tail() {
    // n = 1 com sucesso: P[X >= 1 | 1; 0,9] = 0,9.
    assert_eq!(trials(1, 1).promotion_p_micro(900), 900_000);
    // n = 2 com 2 sucessos: P[X >= 2 | 2; 0,9] = 0,81.
    assert_eq!(trials(2, 2).promotion_p_micro(900), 810_000);
    // n = 10 com 10 sucessos: 0,9^10 = 0,3486784401 -> 348678 micro.
    assert_eq!(trials(10, 10).promotion_p_micro(900), 348_678);
    // Zero sucessos: nada sustenta a promoção.
    assert_eq!(trials(0, 10).promotion_p_micro(900), P_MICRO_SCALE);
}

#[test]
fn the_p_value_shrinks_with_more_honored_trials() {
    let mut previous = P_MICRO_SCALE;
    for total in 1..=60_u32 {
        let p = trials(total, total).promotion_p_micro(900);
        assert!(p < previous, "p não decresceu em n = {total}");
        previous = p;
    }
    // O piso só entra muito longe: 0,9^60 = 0,0018, 0,9^400 = 10⁻¹⁸ (abaixo de 1 micro).
    assert_eq!(previous, 1_797);
    assert_eq!(trials(400, 400).promotion_p_micro(900), P_FLOOR_MICRO);
}

#[test]
fn benjamini_hochberg_rejects_up_to_the_last_significant_rank() {
    // m = 4; rejeita enquanto p <= q·i/m (q = 5 %): 0,0125 · 1, 0,025 · 2, 0,0375 · 3, 0,05 · 4.
    let p = [10_000_u32, 20_000, 60_000, 900_000];
    let control = benjamini_hochberg(&p, 50);
    assert_eq!(control.tested, 4);
    assert_eq!(control.rejected, 2);
    assert_eq!(control.rejections, vec![true, true, false, false]);
}

#[test]
fn benjamini_hochberg_is_a_step_function() {
    // Rejeita o *prefixo* até ao último rank significativo: com m = 3 e q = 5 %, o rank 2 tem
    // folga (0,001 ≤ 0,0333) e o rank 3 não (0,9 > 0,05) — mesmo assim os dois primeiros caem.
    let p = [1_u32, 1_000, 900_000];
    let control = benjamini_hochberg(&p, 50);
    assert_eq!(control.rejected, 2);
    assert_eq!(control.rejections, vec![true, true, false]);
    // O rank 1 sozinho não passaria com m = 3 (folga 0,0167): é o rank 2 que o arrasta.
    assert!(p.first().is_some_and(|value| f64::from(*value) <= 16_700.0));
}

#[test]
fn a_single_tiny_p_value_is_not_enough_in_a_large_family() {
    // O controlo é o que impede a família de 40 regras de passar 40 promoções a 5 %.
    let mut p = vec![P_MICRO_SCALE; 40];
    // Com m = 40, a folga do rank 1 é q/40 = 0,00125: nem um p = 0,02 passa.
    if let Some(slot) = p.get_mut(7) {
        *slot = 20_000;
    }
    let loose = benjamini_hochberg(&p, 50);
    assert_eq!(loose.rejected, 0);
    assert!(loose.rejections.iter().all(|rejected| !rejected));
    if let Some(slot) = p.get_mut(7) {
        *slot = 1_000;
    }
    let control = benjamini_hochberg(&p, 50);
    assert_eq!(
        control.rejected, 1,
        "só a hipótese com p mínimo é rejeitada"
    );
    assert_eq!(
        control
            .rejections
            .iter()
            .filter(|rejected| **rejected)
            .count(),
        1
    );
}

#[test]
fn benjamini_hochberg_is_order_invariant_and_deterministic() {
    let p = [30_000_u32, 5_000, 900_000, 60_000];
    let forward = benjamini_hochberg(&p, 50);
    let reversed_input = [5_000_u32, 900_000, 30_000, 60_000];
    let reversed = benjamini_hochberg(&reversed_input, 50);
    assert_eq!(forward.rejected, reversed.rejected);
    assert_eq!(
        benjamini_hochberg(&p, 50),
        forward,
        "duas execuções, o mesmo resultado"
    );
}

#[test]
fn a_zero_q_rejects_nothing() {
    let p = [1_u32, 1, 1];
    let control = benjamini_hochberg(&p, 0);
    assert_eq!(control.rejected, 0);
    assert!(control.rejections.iter().all(|rejected| !rejected));
}

#[test]
fn an_empty_family_is_well_formed() {
    let control = benjamini_hochberg(&[], 50);
    assert_eq!(control.tested, 0);
    assert_eq!(control.rejected, 0);
    assert!(control.rejections.is_empty());
}

#[test]
fn the_fdr_control_demotes_a_promotion_that_does_not_survive() {
    // Uma regra com 25 ensaios honrados tem LB = 902 >= theta (o limiar promove) mas
    // p = 0,9^25 = 0,0718 > 0,05: a promoção não sobrevive a q = 5 %. É o número que o item
    // impõe: sem FDR a n = 25, com FDR a n = 29.
    let id = RuleId::from("r-fdr");
    let mut verdicts = vec![verdict(&id, trials(25, 25), &Threshold::DEFAULT)];
    assert!(
        verdicts.first().is_some_and(Verdict::is_proven),
        "o limiar sozinho promove"
    );
    let control = control_fdr(&mut verdicts, &Threshold::DEFAULT);
    assert_eq!(control.tested, 1);
    assert_eq!(control.rejected, 0);
    assert_eq!(
        verdicts.first().map(|verdict| verdict.confidence),
        Some(Confidence::Advisory)
    );
    let reason = verdicts
        .first()
        .map(|verdict| verdict.reason.to_string())
        .unwrap_or_default();
    assert!(reason.contains("FDR"), "{reason}");
}

#[test]
fn the_fdr_control_keeps_a_proven_rule() {
    let id = RuleId::from("r-fdr-ok");
    let mut verdicts = vec![verdict(&id, trials(29, 29), &Threshold::DEFAULT)];
    let control = control_fdr(&mut verdicts, &Threshold::DEFAULT);
    assert_eq!(control.rejected, 1);
    assert!(verdicts.first().is_some_and(Verdict::is_proven));
}

#[test]
fn the_fdr_control_never_promotes_what_the_threshold_refused() {
    // Fail-closed: sem ensaios, nem o limiar nem o FDR promovem.
    let mut verdicts = vec![
        verdict(&RuleId::from("r-a"), Trials::new(), &Threshold::DEFAULT),
        verdict(&RuleId::from("r-b"), trials(1, 20), &Threshold::DEFAULT),
    ];
    let control = control_fdr(&mut verdicts, &Threshold::DEFAULT);
    assert_eq!(control.rejected, 0);
    assert!(verdicts.iter().all(|verdict| !verdict.is_proven()));
}
