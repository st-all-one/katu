//! Resumo estatístico determinístico (E18-T10, W7): contagem, percentis, média e IC 95 %.
//!
//! Os gates de performance mediam um p95 único, sem intervalo de confiança nem repetições
//! declaradas ([`docs/profiling.md`](../../../docs/profiling.md) §5). Este módulo é a **fonte
//! única** desse resumo: o [`diag`](crate::diag) agrega-o por `(evento, função)`, o
//! `examples/measure_mvk` publica-o no artefacto cru e o `xtask` usa-o nos gates `gate:render` e
//! `gate:provider`.
//!
//! **Determinismo.** Sem RNG: para `n ≥ 30` o IC 95 % usa a aproximação normal sobre a média
//! (teorema central do limite); para `n` menor usa um *bootstrap* determinístico cujos índices são
//! derivados do próprio `n` ([`splitmix64`]). O mesmo vetor devolve **sempre** o mesmo resumo.
//!
//! **Coerência.** O intervalo é alargado para conter a mediana e a média — um IC que exclui o
//! centro da amostra não é um resumo honesto dela.

use std::fmt;

/// Mínimo de amostras que os gates aceitam (abaixo disto o resumo mente).
pub const MIN_SAMPLES: usize = 5;

/// Reamostragens do *bootstrap* determinístico (`n < 30`).
const RESAMPLES: usize = 2_000;

/// Quantil inferior/f superior do *bootstrap* (em pontos base, IC 95 %).
const BOOT_LOW_BP: u64 = 250;
/// Quantil superior do *bootstrap* (em pontos base, IC 95 %).
const BOOT_HIGH_BP: u64 = 9_750;

/// Z de 95 % bilateral (aproximação normal).
const Z95: f64 = 1.959_963_984_540_054;

/// Amostras insuficientes para um resumo fiável.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooFewSamples {
    /// Amostras observadas.
    pub n: usize,
    /// Mínimo exigido ([`MIN_SAMPLES`]).
    pub min: usize,
}

impl fmt::Display for TooFewSamples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _span = crate::trace_fn!("stats::fmt");
        write!(
            f,
            "amostras insuficientes: n={} < mínimo {}",
            self.n, self.min
        )
    }
}

impl std::error::Error for TooFewSamples {}

/// Resumo determinístico de uma amostra de durações (ns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Número de amostras.
    pub n: u64,
    /// Mediana (percentil 50).
    pub p50: u64,
    /// Percentil 95.
    pub p95: u64,
    /// Média aritmética (arredondada).
    pub mean: u64,
    /// Limite inferior do IC 95 % da média.
    pub ci95_low: u64,
    /// Limite superior do IC 95 % da média.
    pub ci95_high: u64,
}

impl Summary {
    /// Resumo **lenient**: aceita qualquer `n` (vazio ⇒ tudo zero).
    ///
    /// É o que o `diag` usa (nem toda a função atinge [`MIN_SAMPLES`] numa execução); os gates
    /// usam [`Summary::try_from_samples`], que recusa amostras insuficientes.
    #[must_use]
    pub fn from_samples(samples: &[u64]) -> Self {
        let _span = crate::trace_fn!("stats::from_samples");
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self::from_sorted(&sorted)
    }

    /// Resumo para os gates: recusa menos de [`MIN_SAMPLES`] amostras.
    ///
    /// # Erros
    /// Devolve [`TooFewSamples`] quando `samples.len() < MIN_SAMPLES`.
    pub fn try_from_samples(samples: &[u64]) -> Result<Self, TooFewSamples> {
        let _span = crate::trace_fn!("stats::try_from_samples");
        if samples.len() < MIN_SAMPLES {
            return Err(TooFewSamples {
                n: samples.len(),
                min: MIN_SAMPLES,
            });
        }
        Ok(Self::from_samples(samples))
    }

    /// Limites do IC 95 % `(inferior, superior)`.
    #[must_use]
    pub const fn ci95(&self) -> (u64, u64) {
        (self.ci95_low, self.ci95_high)
    }

    /// `true` se o **limite superior** do IC 95 % cabe no orçamento.
    #[must_use]
    pub const fn within_budget(&self, budget: u64) -> bool {
        self.ci95_high <= budget
    }

    /// Resumo de uma amostra já ordenada (evita a segunda ordenação no caminho do `diag`).
    pub(crate) fn from_sorted(sorted: &[u64]) -> Self {
        let _span = crate::trace_fn!("stats::from_sorted");
        let mean = mean(sorted);
        let p50 = percentile(sorted, 5_000);
        let p95 = percentile(sorted, 9_500);
        let (mut ci95_low, mut ci95_high) = interval(sorted);
        // Invariante: o IC contém o centro da amostra (mediana e média).
        ci95_low = ci95_low.min(p50).min(mean);
        ci95_high = ci95_high.max(p50).max(mean);
        Self {
            n: len_u64(sorted.len()),
            p50,
            p95,
            mean,
            ci95_low,
            ci95_high,
        }
    }
}

/// Percentil por *nearest-rank* sobre uma fatia **ordenada**, em pontos base.
///
/// `basis_points` é `50_000` para p50 e `9_500` para p95 (i.e., `p95 = 9500/10000`). Vazio ⇒ `0`.
#[must_use]
pub fn percentile(sorted: &[u64], basis_points: u64) -> u64 {
    let _span = crate::trace_fn!("stats::percentile");
    let n = len_u64(sorted.len());
    if n == 0 {
        return 0;
    }
    // rank = ceil(bp · n / 10_000); índice = rank − 1.
    let rank = basis_points
        .saturating_mul(n)
        .saturating_add(9_999)
        .saturating_div(10_000)
        .max(1);
    let index = usize::try_from(rank.saturating_sub(1)).unwrap_or(usize::MAX);
    sorted
        .get(index)
        .copied()
        .or_else(|| sorted.last().copied())
        .unwrap_or(0)
}

/// Média inteira arredondada ao mais próximo (vazio ⇒ `0`).
fn mean(sorted: &[u64]) -> u64 {
    let _span = crate::trace_fn!("stats::mean");
    if sorted.is_empty() {
        return 0;
    }
    let sum = sorted
        .iter()
        .fold(0_u128, |acc, &value| acc.saturating_add(u128::from(value)));
    let n = u128::from(len_u64(sorted.len()));
    u64::try_from(
        sum.saturating_add(n.saturating_div(2))
            .checked_div(n)
            .unwrap_or(u128::MAX),
    )
    .unwrap_or(u64::MAX)
}

/// IC 95 % da média: normal para `n ≥ 30`, *bootstrap* determinístico abaixo disso.
fn interval(sorted: &[u64]) -> (u64, u64) {
    let _span = crate::trace_fn!("stats::interval");
    match sorted.len() {
        0 => (0, 0),
        n if n >= 30 => normal_interval(sorted),
        _ => bootstrap_interval(sorted),
    }
}

/// IC 95 % pela aproximação normal: `média ± 1,96 · s/√n`.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fronteira do IC: inteiro→f64 só para arredondar; a decisão do gate usa o resultado inteiro"
)]
fn normal_interval(sorted: &[u64]) -> (u64, u64) {
    let _span = crate::trace_fn!("stats::normal_interval");
    let n = f64::from(u32::try_from(sorted.len()).unwrap_or(u32::MAX));
    let mean = sorted.iter().map(|&value| value as f64).sum::<f64>() / n;
    if sorted.len() < 2 {
        let value = saturate(mean);
        return (value, value);
    }
    let variance = sorted
        .iter()
        .map(|&value| {
            let delta = value as f64 - mean;
            delta * delta
        })
        .sum::<f64>()
        / (n - 1.0);
    let margin = Z95 * (variance / n).sqrt();
    (saturate(mean - margin), saturate(mean + margin))
}

/// IC 95 % por *bootstrap* determinístico (sem RNG): índices derivados de `n`.
fn bootstrap_interval(sorted: &[u64]) -> (u64, u64) {
    let _span = crate::trace_fn!("stats::bootstrap_interval");
    let n = u64::try_from(sorted.len()).unwrap_or(u64::MAX);
    if n == 0 {
        return (0, 0);
    }
    let mut state = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut means = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let mut sum: u128 = 0;
        for _ in 0..n {
            let draw = splitmix64(&mut state);
            let index = usize::try_from(draw.checked_rem(n).unwrap_or(0)).unwrap_or(0);
            if let Some(&value) = sorted.get(index) {
                sum = sum.saturating_add(u128::from(value));
            }
        }
        means.push(
            u64::try_from(sum.checked_div(u128::from(n)).unwrap_or(u128::MAX)).unwrap_or(u64::MAX),
        );
    }
    means.sort_unstable();
    (
        percentile(&means, BOOT_LOW_BP),
        percentile(&means, BOOT_HIGH_BP),
    )
}

/// `SplitMix64` (constantes públicas do algoritmo); determinístico por semente.
///
/// `const fn`: é chamado milhares de vezes dentro do *bootstrap*, onde um span seria ruído puro.
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `f64` → `u64` arredondado, saturando em `0`/`u64::MAX` (o IC nunca rebenta o tipo).
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fronteira do IC: f64 finito→u64 saturado; o valor já foi validado antes da conversão"
)]
fn saturate(value: f64) -> u64 {
    let _span = crate::trace_fn!("stats::saturate");
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    let max = u64::MAX as f64;
    if value >= max {
        return u64::MAX;
    }
    value.round() as u64
}

/// Comprimento como `u64` (saturando).
fn len_u64(len: usize) -> u64 {
    let _span = crate::trace_fn!("stats::len_u64");
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
