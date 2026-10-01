//! Testes da confiança medida (Q-11): limites do posterior, limiar, demolição e determinismo.

use super::{Confidence, Threshold, Trials, verdict};
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
    assert!(!after.contradiction, "um registo perfeito não contradiz nada");
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
