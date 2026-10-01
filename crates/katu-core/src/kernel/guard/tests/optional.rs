//! Erro tipo I **exato** sob parada opcional (C1/W8-3).
//!
//! A e-value `Λ_n` é um martingale não-negativo sob `H0`; por **Ville**,
//! `P_{H0}(∃ n: log Λ_n ≥ b) ≤ e^{−b}` para **qualquer** regra de parada. Aqui calcula-se esse
//! supremo de forma **exata** por programação dinâmica sobre `(passos, repetições)` — sem RNG, sem
//! simulação — para travar a propriedade e contrastar com a fronteira nominal do SPRT.
//!
//! O estado é `(m, k)`: `m` passos observados, `k` deles inteiramente repetidos. O log da e-value
//! é `m·b_novo + k·(b_rep − b_novo)` (só depende de `m` e `k`), pelo que a probabilidade de nunca
//! cruzar a fronteira `b` cabe numa DP `O(n²)`.

use super::super::{GuardParams, log_ratio, to_milli};

/// Log-incremento de um passo inteiramente repetido (`log(p₁/p₀)`), em milésimos.
fn repeat_step(params: GuardParams) -> i64 {
    let _span = crate::trace_fn!("kernel::guard::tests::optional::repeat_step");

    log_ratio(params.e_value_p1_milli, params.e_value_p0_milli)
}

/// Log-incremento de um passo com novidade (`log((1−p₁)/(1−p₀))`), em milésimos.
fn novel_step(params: GuardParams) -> i64 {
    let _span = crate::trace_fn!("kernel::guard::tests::optional::novel_step");

    log_ratio(
        1_000_u32.saturating_sub(params.e_value_p1_milli),
        1_000_u32.saturating_sub(params.e_value_p0_milli),
    )
}

/// `P_{H0}(∃ m ≤ n: log Λ_m ≥ boundary)` em milésimos, **exato** sob `H0` (iid `p₀`).
///
/// DP determinística `O(n²)`: `prev[k]` é a probabilidade de estar em `(m−1, k)` **sem** ter
/// cruzado; uma transição cujo `(m, k)` cruza a fronteira é descartada (essa massa já rejeitou).
#[must_use]
pub(super) fn error_milli(params: GuardParams, boundary_milli: i64, n: u32) -> u32 {
    let p0 = f64::from(params.e_value_p0_milli) / 1_000.0;
    let repeat = repeat_step(params);
    let novel = novel_step(params);
    let span = repeat.saturating_sub(novel);
    let width = usize::try_from(n).unwrap_or(0).saturating_add(1);
    let mut prev = vec![0.0_f64; width];
    if let Some(slot) = prev.get_mut(0) {
        *slot = 1.0;
    }
    for m in 1..=n {
        let mut cur = vec![0.0_f64; width];
        for k in 0..=m {
            let log_lr = i64::from(m)
                .saturating_mul(novel)
                .saturating_add(i64::from(k).saturating_mul(span));
            if log_lr >= boundary_milli {
                continue;
            }
            let index = usize::try_from(k).unwrap_or(0);
            let from_novel = prev.get(index).copied().unwrap_or(0.0);
            let from_repeat = k
                .checked_sub(1)
                .and_then(|j| prev.get(usize::try_from(j).unwrap_or(0)).copied())
                .unwrap_or(0.0);
            let p = from_novel.mul_add(1.0 - p0, from_repeat * p0);
            if let Some(slot) = cur.get_mut(index) {
                *slot = p;
            }
        }
        prev = cur;
    }
    to_permille(1.0 - prev.iter().sum::<f64>())
}

/// Converte uma probabilidade em `[0, 1]` para milésimos, saturando.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "probabilidade em [0,1] → milésimos; o valor é limitado e arredondado (precedente: `guard::to_milli`)"
)]
fn to_permille(value: f64) -> u32 {
    let scaled = (value * 1_000.0).clamp(0.0, 1_000.0);
    scaled.round() as u32
}

/// Fronteira **nominal** do SPRT clássico `log((1−β)/α)` (só para contraste).
pub(super) fn sprt_boundary_milli(params: GuardParams, beta_milli: u32) -> i64 {
    let _span = crate::trace_fn!("kernel::guard::tests::optional::sprt_boundary_milli");

    let alpha = f64::from(params.e_value_alpha_milli.max(1)) / 1_000.0;
    let beta = f64::from(beta_milli) / 1_000.0;
    to_milli(((1.0 - beta) / alpha).ln())
}

#[test]
fn the_e_value_covers_any_stopping_time() {
    let params = GuardParams::DEFAULT;
    let boundary = params.e_value_threshold_milli();
    for n in [1_u32, 2, 3, 5, 10, 20, 50, 100, 200, 500] {
        let error = error_milli(params, boundary, n);
        assert!(
            error <= params.e_value_alpha_milli,
            "n = {n}: {error}‰ > {}‰ (a cobertura anytime-valid falhou)",
            params.e_value_alpha_milli
        );
    }
}

#[test]
fn the_nominal_sprt_boundary_is_less_conservative() {
    let params = GuardParams::DEFAULT;
    let nominal = sprt_boundary_milli(params, 100);
    // A fronteira nominal do SPRT fica **abaixo** do limiar de Ville; é mais rápida, mas a
    // garantia teórica é `α/(1−β) > α`, não `α`.
    assert!(nominal < params.e_value_threshold_milli());
    let sprt_bound = f64::from(params.e_value_alpha_milli) / (1.0 - 0.10);
    assert!(sprt_bound > f64::from(params.e_value_alpha_milli));
    // Na gama de produção ambas ficam ≤ α; a e-value é a mais conservadora.
    let e_value_error = error_milli(params, params.e_value_threshold_milli(), 500);
    let sprt_error = error_milli(params, nominal, 500);
    assert!(
        e_value_error <= sprt_error,
        "a e-value é a mais conservadora"
    );
    assert!(sprt_error <= params.e_value_alpha_milli);
}

#[test]
fn the_optional_stopping_error_is_deterministic() {
    let params = GuardParams::DEFAULT;
    let boundary = params.e_value_threshold_milli();
    assert_eq!(
        error_milli(params, boundary, 100),
        error_milli(params, boundary, 100)
    );
}

#[test]
fn the_error_grows_with_the_horizon() {
    let params = GuardParams::DEFAULT;
    let boundary = params.e_value_threshold_milli();
    let short = error_milli(params, boundary, 4);
    let long = error_milli(params, boundary, 200);
    assert!(short <= long, "o erro acumula com o horizonte");
}
