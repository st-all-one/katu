//! Controlo de múltiplas comparações (C5): p-value exacto unilateral por regra e
//! Benjamini–Hochberg sobre a família de regras `Enforced`.
//!
//! O limite inferior de Wilson (Q-11) é **unilateral por regra**: com `m` regras testadas ao mesmo
//! nível, o erro de tipo I da família cresce como `~α·m` (a 5 %, `m = 20` daria ~1 promoção falsa
//! em média). O BH troca esse `α·m` por um `q` para a família inteira, ao preço de exigir mais
//! evidência por regra quando a família cresce.
//!
//! O p-value é o **exacto** unilateral (`H0: p ≥ θ`), com a cauda binomial superior
//! `P[X ≥ s | n, θ]` — não o Wald nem o LB — em **micro** (1e-6, piso 1 micro), porque o valor útil
//! vive longe de 1 % (um registo perfeito a `θ = 0,9` dá `0,9ⁿ`).
//!
//! `control_fdr` é **fail-closed**: só pode tirar promoções, nunca dar. O motivo do veredicto fica
//! com a evidência (p, q), para que a demoção não seja opaca.

use crate::decision::Reason;

use super::{Confidence, Threshold, Trials, Verdict};

#[cfg(test)]
mod tests;

/// Escala dos p-valores: **micro** (1e-6), para que o piso de 1 micro seja legível.
pub const P_MICRO_SCALE: u32 = 1_000_000;

/// Piso do p-value em micro (1e-6): abaixo disso dizemos "muito pequeno", nunca zero.
pub const P_FLOOR_MICRO: u32 = 1;

impl Trials {
    /// p-value **unilateral** da afirmação "a regra sustenta-se em `θ`" (micro).
    ///
    /// `H0: p ≥ θ` contra `H1: p < θ`, com o exacto binomial (`P[X ≥ s | n, θ]`): é o teste
    /// exacto que sustenta a **promoção** a `Enforced`. Micro e não fração porque o valor útil
    /// vive longe de 1 % (um registo perfeito a `θ = 0,9` dá `p = 0,9ⁿ`).
    ///
    /// Sem observações não há evidência em qualquer sentido: `p = 1` (nada a promover).
    #[must_use]
    pub fn promotion_p_micro(self, theta_milli: u32) -> u32 {
        if self.trials == 0 {
            return P_MICRO_SCALE;
        }
        let theta = f64::from(theta_milli) / 1_000.0;
        to_micro(binomial_upper_tail(self.trials, self.successes, theta))
    }
}

/// `P[X ≥ s | n, θ]` — cauda **superior** binomial, por termos recursivos.
///
/// É a cauda certa para `H0: p ≥ θ`: sob a hipótese nula, observar `s` ou mais honras é o que a
/// torna improvável. Os termos são gerados de `k = n` para baixo (`P[X = n] = θⁿ`), o que evita
/// `powi` para expoentes grandes e mantém a soma estável para `n` grande.
fn binomial_upper_tail(n: u32, successes: u32, theta: f64) -> f64 {
    let s = successes.min(n);
    if s == 0 {
        return 1.0;
    }
    if theta <= 0.0 {
        return 0.0;
    }
    let odds = (1.0 - theta) / theta;
    let mut term = theta.powi(i32::try_from(n).unwrap_or(i32::MAX));
    let mut total = term;
    for k in (s..n).rev() {
        let k_f = f64::from(k);
        term *= (k_f / f64::from(k.saturating_add(1))) * odds;
        total += term;
    }
    total.clamp(0.0, 1.0)
}

/// `usize` → `f64` sem `as` (os lints do workspace proíbem a conversão silenciosa).
fn to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

/// Converte uma fração `[0, 1]` em micro de p-value (piso em [`P_FLOOR_MICRO`]).
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fração já limitada a [0,1]; o arredondamento em micro é o pretendido (precedente: `to_milli`)"
)]
fn to_micro(value: f64) -> u32 {
    let floor = f64::from(P_FLOOR_MICRO);
    let scaled = (value.clamp(0.0, 1.0) * f64::from(P_MICRO_SCALE)).round();
    if scaled <= floor {
        return P_FLOOR_MICRO;
    }
    scaled as u32
}

/// Resultado de um controlo de múltiplas comparações (C5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipleTests {
    /// `q` pedido (milésimos: FDR alvo).
    pub q_milli: u32,
    /// Nº de hipóteses na família.
    pub tested: usize,
    /// Nº de `H0` **rejeitadas** (as promoções que sobrevivem ao controlo).
    pub rejected: usize,
    /// Rejeição por hipótese, **na ordem de entrada** (determinístico).
    pub rejections: Vec<bool>,
}

/// Benjamini–Hochberg (C5): rejeita `H0_(i)` para todo o `i ≤ k`, onde `k` é o maior índice com
/// `p_(i) ≤ q·i/m`.
///
/// É o controlo certo para *promoções*: com `m` regras testadas ao mesmo nível, o risco de
/// promover uma regra que não se sustenta deixa de ser `α` por regra e passa a ser `q` para a
/// família inteira (FDR). Sem ele, `m = 20` regras a 5 % dariam ~1 promoção falsa em média.
///
/// Determinístico: a ordenação desempata pelo índice, logo a mesma família dá sempre as mesmas
/// rejeições, independentemente da ordem de entrada.
#[must_use]
pub fn benjamini_hochberg(p_micro: &[u32], q_milli: u32) -> MultipleTests {
    let m = p_micro.len();
    let mut rejections = vec![false; m];
    if m == 0 || q_milli == 0 {
        return MultipleTests {
            q_milli,
            tested: m,
            rejected: 0,
            rejections,
        };
    }
    // Ordena por (p, índice): a ligação é determinística mesmo com p-valores iguais.
    let mut order: Vec<(u32, usize)> = p_micro
        .iter()
        .copied()
        .enumerate()
        .map(|(position, p)| (p, position))
        .collect();
    order.sort_unstable();
    let q = f64::from(q_milli) / 1_000.0;
    let mut last = 0_usize;
    for (index, (p_micro_value, _)) in order.iter().enumerate() {
        let rank = index.saturating_add(1);
        let bound = q * to_f64(rank) / to_f64(m);
        let p = f64::from(*p_micro_value) / f64::from(P_MICRO_SCALE);
        if p <= bound {
            last = rank;
        }
    }
    for &(_, position) in order.iter().take(last) {
        if let Some(slot) = rejections.get_mut(position) {
            *slot = true;
        }
    }
    MultipleTests {
        q_milli,
        tested: m,
        rejected: last,
        rejections,
    }
}

/// Aplica o controlo de múltiplas comparações (C5) a uma **família** de veredictos.
///
/// Cada `Enforced` é uma afirmação estatística ("esta regra sustenta-se em `θ`"); com `m` regras testadas
/// ao mesmo nível, o teste unilateral por regra deixa passar ~`α·m` promoções falsos. Aqui as `m`
/// afirmações entram no Benjamini–Hochberg com `q = threshold.q_milli`, e a promoção que **não**
/// sobrevive é demovida a `Advisory` com a evidência no motivo.
///
/// Fail-closed: só pode **tirar** promoções, nunca dar promoções. Com `m = 1` o controlo é o
/// próprio teste unilateral (`p ≤ q`), ou seja, o limiar prático é o mesmo — o ganho aparece nas
/// famílias grandes, que é onde o `α·m` morde.
pub fn control_fdr(verdicts: &mut [Verdict], threshold: &Threshold) -> MultipleTests {
    let p_micro: Vec<u32> = verdicts
        .iter()
        .map(|verdict| {
            Trials::from_parts(verdict.successes, verdict.trials)
                .promotion_p_micro(threshold.theta_milli)
        })
        .collect();
    let control = benjamini_hochberg(&p_micro, threshold.q_milli);
    for (verdict, survived) in verdicts.iter_mut().zip(control.rejections.iter().copied()) {
        if verdict.confidence != Confidence::Enforced || survived {
            continue;
        }
        let p = Trials::from_parts(verdict.successes, verdict.trials)
            .promotion_p_micro(threshold.theta_milli);
        verdict.confidence = Confidence::Advisory;
        verdict.reason = Reason::new(format!(
            "{verdict_reason}; promoção não sobrevive ao controlo FDR (p = {p} micro, q = {q}‰): rebaixada a Advisory",
            verdict_reason = verdict.reason,
            q = threshold.q_milli,
        ));
    }
    control
}
