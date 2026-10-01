//! Testes da confiança medida (Q-11) e do controlo de múltiplas comparações (C5): limites do
//! posterior, limiar, demolição, Benjamini–Hochberg e determinismo.

use super::{CALIBRATION_BINS, Confidence, Threshold, Trials, calibrate, verdict};
use crate::rule::RuleId;

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
fn the_empty_accumulator_is_halved_by_the_prior() {
    let empty = Trials::new();
    assert_eq!(empty.trials(), 0);
    assert_eq!(empty.successes(), 0);
    assert_eq!(empty.mean_milli(), 500);
    assert_eq!(empty.wilson_lower_milli(1_645), 0);
}

#[test]
fn the_wilson_bound_never_exceeds_the_mean() {
    for total in 1..=40_u32 {
        for successes in 0..=total {
            let accumulator = trials(successes, total);
            let lower = accumulator.wilson_lower_milli(1_645);
            assert!(
                lower <= accumulator.mean_milli(),
                "LB {lower} > média {} (n = {total}, s = {successes})",
                accumulator.mean_milli()
            );
        }
    }
}

#[test]
fn the_wilson_bound_is_monotone_in_successes() {
    for total in 1..=40_u32 {
        let mut previous = 0;
        for successes in 0..=total {
            let lower = trials(successes, total).wilson_lower_milli(1_645);
            assert!(lower >= previous, "LB caiu ao subir os sucessos");
            previous = lower;
        }
    }
}

#[test]
fn a_perfect_record_proves_enforced_at_twenty_five_observations() {
    let id = RuleId::from("r-perfeita");
    let before = verdict(&id, trials(24, 24), &Threshold::DEFAULT);
    let after = verdict(&id, trials(25, 25), &Threshold::DEFAULT);
    assert_eq!(before.confidence, Confidence::Advisory);
    assert_eq!(before.lower_milli, 899);
    assert_eq!(after.confidence, Confidence::Enforced);
    assert_eq!(after.lower_milli, 902);
    assert!(
        !after.contradiction,
        "um registo perfeito não contradiz nada"
    );
    assert!(after.is_proven());
}

#[test]
fn one_violation_demolishes_the_rule() {
    let id = RuleId::from("r-demolida");
    let clean = verdict(&id, trials(20, 20), &Threshold::DEFAULT);
    let dirty = verdict(&id, trials(19, 20), &Threshold::DEFAULT);
    assert_eq!(clean.confidence, Confidence::Advisory);
    assert_eq!(clean.lower_milli, 881);
    assert_eq!(dirty.confidence, Confidence::Advisory);
    assert_eq!(dirty.lower_milli, 804);
    assert!(
        dirty.contradiction,
        "n ≥ n_min com uma falha é contradição medida"
    );
    assert!(dirty.reason.as_str().contains("1 falhas em 20 ensaios"));
}

#[test]
fn few_observations_are_not_a_contradiction() {
    let id = RuleId::from("r-nova");
    let fresh = verdict(&id, trials(2, 2), &Threshold::DEFAULT);
    assert_eq!(fresh.confidence, Confidence::Advisory);
    assert!(!fresh.contradiction);
    assert!(fresh.reason.as_str().contains("ainda não provado"));
    let empty = verdict(&id, Trials::new(), &Threshold::DEFAULT);
    assert_eq!(empty.confidence, Confidence::Unmeasured);
    assert!(!empty.contradiction);
    assert!(empty.reason.as_str().contains("não medida"));
}

#[test]
fn the_verdict_is_a_pure_function_of_the_observations() {
    let id = RuleId::from("r-determinista");
    let first = verdict(&id, trials(7, 9), &Threshold::DEFAULT);
    let second = verdict(&id, trials(7, 9), &Threshold::DEFAULT);
    assert_eq!(first, second);
    // A ordem das observações não muda o acumulador (só a contagem).
    let mut shuffled = Trials::new();
    shuffled.observe_honored();
    shuffled.observe_violation();
    shuffled.observe_honored();
    assert_eq!(shuffled, trials(2, 3));
}

#[test]
fn a_stricter_threshold_demotes_what_the_default_proves() {
    let id = RuleId::from("r-limiar");
    let strict = Threshold {
        theta_milli: 990,
        ..Threshold::DEFAULT
    };
    assert_eq!(
        verdict(&id, trials(25, 25), &Threshold::DEFAULT).confidence,
        Confidence::Enforced
    );
    assert_eq!(
        verdict(&id, trials(25, 25), &strict).confidence,
        Confidence::Advisory
    );
}

#[test]
fn merging_accumulators_sums_the_trials() {
    let mut left = trials(3, 4);
    left.merge(trials(5, 6));
    assert_eq!(left.trials(), 10);
    assert_eq!(left.successes(), 8);
    assert_eq!(Confidence::Enforced.as_str(), "enforced");
    assert_eq!(Confidence::Advisory.as_str(), "advisory");
    assert_eq!(Confidence::Unmeasured.as_str(), "unmeasured");
}

#[test]
fn an_empty_calibration_has_no_bins() {
    let calibration = calibrate(&[]);
    assert_eq!(calibration.trials, 0);
    assert_eq!(calibration.brier_milli, 0);
    assert_eq!(calibration.ece_milli, 0);
    assert!(calibration.bins.is_empty());
    // `unmeasured` (n = 0) não calibra nada.
    let unmeasured = verdict(&RuleId::from("r-vazia"), Trials::new(), &Threshold::DEFAULT);
    assert_eq!(calibrate(&[unmeasured]).trials, 0);
}

#[test]
fn the_ece_decreases_as_the_evidence_accumulates() {
    let id = RuleId::from("r-calibrada");
    let mut previous = u32::MAX;
    for total in 1..=25_u32 {
        let one = verdict(&id, trials(total, total), &Threshold::DEFAULT);
        let calibration = calibrate(&[one]);
        assert!(
            calibration.ece_milli <= previous,
            "ECE subiu em n = {total}: {} > {previous}",
            calibration.ece_milli
        );
        previous = calibration.ece_milli;
    }
    assert!(previous < 100, "o LB a n = 25 já está perto da frequência");
}

#[test]
fn a_violation_worsens_the_calibration() {
    let id = RuleId::from("r-calibrada");
    let clean = calibrate(&[verdict(&id, trials(20, 20), &Threshold::DEFAULT)]);
    let dirty = calibrate(&[verdict(&id, trials(19, 20), &Threshold::DEFAULT)]);
    assert!(
        dirty.ece_milli > clean.ece_milli,
        "a violação devia afastar o LB da frequência"
    );
    assert!(dirty.brier_milli > clean.brier_milli);
}

#[test]
fn the_calibration_is_a_pure_function_of_the_verdicts() {
    let id = RuleId::from("r-calibrada");
    let verdicts = [
        verdict(&id, trials(25, 25), &Threshold::DEFAULT),
        verdict(&id, trials(19, 20), &Threshold::DEFAULT),
    ];
    let first = calibrate(&verdicts);
    let second = calibrate(&verdicts);
    assert_eq!(first, second);
    assert!(first.brier_milli <= 1_000);
    assert!(first.ece_milli <= 1_000);
}

#[test]
fn the_reliability_bins_partition_the_trials() {
    let id = RuleId::from("r-calibrada");
    let verdicts = [
        verdict(&id, trials(25, 25), &Threshold::DEFAULT),
        verdict(&id, trials(19, 20), &Threshold::DEFAULT),
        verdict(&id, trials(4, 5), &Threshold::DEFAULT),
    ];
    let calibration = calibrate(&verdicts);
    assert_eq!(
        calibration.bins.len(),
        usize::try_from(CALIBRATION_BINS).unwrap_or(0)
    );
    let sum: u32 = calibration.bins.iter().map(|bin| bin.trials).sum();
    assert_eq!(sum, calibration.trials);
    for bin in &calibration.bins {
        assert!(bin.upper_milli > bin.lower_milli);
        assert!(bin.predicted_milli <= 1_000);
        assert!(bin.observed_milli <= 1_000);
    }
}
