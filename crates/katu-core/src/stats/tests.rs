//! Testes do resumo estatístico (E18-T10/W7).

mod conformal_bench;

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

#[test]
fn mad_is_robust_to_outliers() {
    // C7: o MAD não se move com um outlier; a média sim.
    let clean: Vec<u64> = (1..=20).collect();
    let mut with_outlier = clean.clone();
    with_outlier.push(1_000_000);
    let clean_summary = Summary::from_samples(&clean);
    let outlier_summary = Summary::from_samples(&with_outlier);
    // O MAD da amostra com outlier é pequeno (a maioria dos valores é 1..20).
    assert!(
        outlier_summary.mad <= 10,
        "MAD não é robusto: {}",
        outlier_summary.mad
    );
    // A média, essa, dispara.
    assert!(
        outlier_summary.mean > clean_summary.mean * 10,
        "a média devia disparar com o outlier"
    );
}

#[test]
fn the_robust_interval_contains_the_median() {
    let samples: Vec<u64> = (1..=100).collect();
    let summary = Summary::from_samples(&samples);
    let (low, high) = summary.robust_ci95();
    assert!(
        low <= summary.p50 && summary.p50 <= high,
        "IC robusto não contém a mediana: {summary:?}"
    );
    assert!(low <= high);
}

#[test]
fn the_robust_interval_is_stable_under_outliers() {
    // C7: o IC robusto não se move com um outlier; o IC da média sim.
    let clean: Vec<u64> = (1..=20).collect();
    let mut with_outlier = clean.clone();
    with_outlier.push(1_000_000);
    let clean_summary = Summary::from_samples(&clean);
    let outlier_summary = Summary::from_samples(&with_outlier);
    let (clean_low, clean_high) = clean_summary.robust_ci95();
    let (outlier_low, outlier_high) = outlier_summary.robust_ci95();
    // O IC robusto é resistente: a diferença é pequena.
    assert!(
        outlier_high.abs_diff(clean_high) < clean_high / 2,
        "IC robusto não é robusto: ({clean_low}, {clean_high}) vs ({outlier_low}, {outlier_high})"
    );
}

#[test]
fn empty_and_single_samples_have_zero_mad() {
    let empty = Summary::from_samples(&[]);
    assert_eq!(empty.mad, 0);
    assert_eq!(empty.robust_ci95(), (0, 0));
    let single = Summary::from_samples(&[42]);
    assert_eq!(single.mad, 0);
    assert_eq!(single.robust_ci95(), (42, 42));
}

/// A/B determinístico da estatística robusta (C7): escreve o artefacto em `KATU_STATS_OUT`.
#[test]
#[ignore = "bench A/B: escreve o artefacto do protocolo (a via normal é o gate)"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_stats_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;
    let clean: Vec<u64> = (1..=20).collect();
    let mut with_outlier = clean.clone();
    with_outlier.push(1_000_000);
    let clean_summary = Summary::from_samples(&clean);
    let outlier_summary = Summary::from_samples(&with_outlier);

    let mad_robust = outlier_summary.mad <= 10;
    let mean_not_robust = outlier_summary.mean > clean_summary.mean * 10;
    let (r_low, r_high) = outlier_summary.robust_ci95();
    let robust_contains_median = r_low <= outlier_summary.p50 && outlier_summary.p50 <= r_high;
    let (c_low, c_high) = clean_summary.robust_ci95();
    let robust_stable = r_high.abs_diff(c_high) < c_high / 2;

    let criteria = [
        ("mad_robust_to_outliers", mad_robust),
        ("mean_not_robust", mean_not_robust),
        ("robust_ci_contains_median", robust_contains_median),
        ("robust_ci_stable_under_outliers", robust_stable),
    ];
    let met = criteria.iter().filter(|(_, ok)| *ok).count();

    if let Ok(path) = std::env::var("KATU_STATS_OUT") {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut report = String::from("{\n");
        report.push_str("  \"c7_stats_robust\": {\n");
        let clean_p50 = clean_summary.p50;
        let clean_mean = clean_summary.mean;
        let clean_mad = clean_summary.mad;
        let outlier_p50 = outlier_summary.p50;
        let outlier_mean = outlier_summary.mean;
        let outlier_mad = outlier_summary.mad;
        writeln!(
            report,
            "    \"clean_p50\": {clean_p50}, \"clean_mean\": {clean_mean}, \"clean_mad\": {clean_mad}, \"clean_robust_ci95\": [{c_low}, {c_high}],"
        )?;
        writeln!(
            report,
            "    \"outlier_p50\": {outlier_p50}, \"outlier_mean\": {outlier_mean}, \"outlier_mad\": {outlier_mad}, \"outlier_robust_ci95\": [{r_low}, {r_high}],"
        )?;
        report.push_str("    \"criteria\": [\n");
        for (i, (name, ok)) in criteria.iter().enumerate() {
            let comma = if i + 1 < criteria.len() { "," } else { "" };
            writeln!(
                report,
                "      {{\"name\": \"{name}\", \"met\": {ok}}}{comma}"
            )?;
        }
        report.push_str("    ]\n  }\n}\n");
        std::fs::write(path, report)?;
    }

    assert_eq!(met, 4, "nem todos os critérios cumpridos: {criteria:?}");
    Ok(())
}
