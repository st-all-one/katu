//! Testes do resumo estatístico (E18-T10/W7).

use super::{MIN_SAMPLES, Summary, TooFewSamples, percentile};

#[test]
fn too_few_samples_is_an_error() {
    let samples = [1_u64, 2, 3, 4];
    let error = Summary::try_from_samples(&samples).err();
    assert_eq!(
        error,
        Some(TooFewSamples {
            n: 4,
            min: MIN_SAMPLES
        })
    );
    assert!(Summary::try_from_samples(&[0; MIN_SAMPLES]).is_ok());
    assert_eq!(Summary::from_samples(&samples).n, 4);
}

#[test]
fn percentile_is_nearest_rank() {
    let sorted: Vec<u64> = (1..=100).collect();
    assert_eq!(percentile(&sorted, 5_000), 50);
    assert_eq!(percentile(&sorted, 9_500), 95);
    assert_eq!(percentile(&[], 5_000), 0);
}

#[test]
fn the_interval_contains_the_median_and_the_mean() {
    // Grande (normal) e pequeno (bootstrap).
    let large: Vec<u64> = (1..=200).collect();
    let small = [10_u64, 20, 30, 40, 50];
    for samples in [large.as_slice(), small.as_slice()] {
        let summary = Summary::from_samples(samples);
        assert!(
            summary.ci95_low <= summary.p50 && summary.p50 <= summary.ci95_high,
            "IC não contém a mediana: {summary:?}"
        );
        assert!(
            summary.ci95_low <= summary.mean && summary.mean <= summary.ci95_high,
            "IC não contém a média: {summary:?}"
        );
        assert!(summary.ci95_low <= summary.ci95_high);
    }
}

#[test]
fn the_summary_is_deterministic() {
    let samples = [7_u64, 3, 9, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9];
    assert_eq!(
        Summary::from_samples(&samples),
        Summary::from_samples(&samples)
    );
}

#[test]
fn a_regression_above_the_budget_fails() {
    let summary = Summary::from_samples(&[100, 100, 100, 100, 100]);
    assert!(summary.within_budget(100));
    assert!(!summary.within_budget(99));
}

#[test]
fn empty_and_single_samples_are_stable() {
    let empty = Summary::from_samples(&[]);
    assert_eq!(empty.n, 0);
    assert_eq!(empty.ci95(), (0, 0));
    let single = Summary::from_samples(&[42]);
    assert_eq!(single.n, 1);
    assert_eq!(single.p50, 42);
    assert_eq!(single.p95, 42);
    assert_eq!(single.mean, 42);
    assert_eq!(single.ci95(), (42, 42));
}

#[test]
fn the_bootstrap_is_a_function_of_n_alone() {
    // Mesmo `n` e mesma amostra ⇒ mesma cauda; semente derivada de `n` (sem RNG).
    let a = [1_u64, 50, 3, 90, 7, 12, 44];
    let b = [1_u64, 50, 3, 90, 7, 12, 44];
    assert_eq!(
        Summary::from_samples(&a).ci95(),
        Summary::from_samples(&b).ci95()
    );
}
