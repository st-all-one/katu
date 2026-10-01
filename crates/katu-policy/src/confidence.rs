//! Confiança **medida** de uma regra (Q-11/F6): posterior Beta–Bernoulli e limite inferior de
//! Wilson.
//!
//! `Enforced` é uma categoria **declarada** no TOML; o `OPTIMIZATION_PLAN.md` §1.4-i exige que seja
//! **provada**. A estatística vive aqui, sem I/O (o chamador extrai as observações do log —
//! `katu-policy` é instrumentada pelo chamador): o ensaio de uma regra é *a negação foi honrada* e o
//! veredicto só mantém `Enforced` com `LB ≥ θ` **e** `n ≥ n_min`; senão demove a `Advisory` **com a
//! evidência** (`trials`, `successes`, `lower_milli`) no próprio motivo.
//!
//! O limiar é **dado** ([`Threshold::DEFAULT`]), nunca derivado dos dados: nada aqui escala política
//! ou modelo sozinho (DF8). A conta é determinística (IEEE-754, sem RNG, sem relógio).

use crate::decision::Reason;
use crate::rule::RuleId;

/// Contagem de ensaios de Bernoulli com prior uniforme: posterior `Beta(1 + s, 1 + n − s)`.
///
/// O prior uniforme evita `p = 1` com uma amostra pequena: com `n = 0` a média posterior é `1/2`,
/// não `1`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trials {
    successes: u32,
    trials: u32,
}

impl Trials {
    /// Acumulador vazio.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            successes: 0,
            trials: 0,
        }
    }

    /// Registra um ensaio **honrado** (a regra fez o que prometia).
    pub const fn observe_honored(&mut self) {
        self.trials = self.trials.saturating_add(1);
        self.successes = self.successes.saturating_add(1);
    }

    /// Registra um ensaio **violado** (a ação proibida aconteceu).
    pub const fn observe_violation(&mut self) {
        self.trials = self.trials.saturating_add(1);
    }

    /// Sucessos observados.
    #[must_use]
    pub const fn successes(self) -> u32 {
        self.successes
    }

    /// Ensaios observados.
    #[must_use]
    pub const fn trials(self) -> u32 {
        self.trials
    }

    /// Soma outro acumulador (retoma de vários logs).
    pub const fn merge(&mut self, other: Self) {
        self.successes = self.successes.saturating_add(other.successes);
        self.trials = self.trials.saturating_add(other.trials);
    }

    /// Média posterior em milésimos: `(1 + s) / (2 + n)`.
    #[must_use]
    pub fn mean_milli(self) -> u32 {
        let numerator = self.successes.saturating_add(1);
        let denominator = self.trials.saturating_add(2);
        numerator
            .saturating_mul(1_000)
            .checked_div(denominator)
            .unwrap_or(0)
    }

    /// Limite inferior do intervalo de Wilson (unilateral), em milésimos.
    ///
    /// `LB = (p̂ + z²/2n − z·√(p̂(1−p̂)/n + z²/4n²)) / (1 + z²/n)`, com `z` em milésimos
    /// (`1 645` ≈ 95 % unilateral). Com `n = 0` não há intervalo: devolve `0`.
    #[must_use]
    pub fn wilson_lower_milli(self, z_milli: u32) -> u32 {
        if self.trials == 0 {
            return 0;
        }
        let n = f64::from(self.trials);
        let successes = f64::from(self.successes);
        let p = successes / n;
        let z = f64::from(z_milli) / 1_000.0;
        let z_squared = z * z;
        let center = p + z_squared / (2.0 * n);
        let margin = z * (p * (1.0 - p) / n + z_squared / (4.0 * n * n)).sqrt();
        to_milli((center - margin) / (1.0 + z_squared / n))
    }
}

/// Converte uma fração `[0, 1]` em milésimos (satura fora do intervalo e em `NaN`).
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fração já limitada a [0,1]; o arredondamento é o pretendido (precedente: `evidence::from_f64`)"
)]
fn to_milli(value: f64) -> u32 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * 1_000.0).round() as u32
}

/// Limiar de decisão: **dado**, não derivado dos dados (DF8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold {
    /// Probabilidade mínima exigida ao limite inferior, em milésimos.
    pub theta_milli: u32,
    /// Ensaios mínimos para decidir (abaixo disto o veredicto é *não provado*).
    pub n_min: u32,
    /// Quantil normal unilateral em milésimos (`1 645` ≈ 95 %).
    pub z_milli: u32,
}

impl Threshold {
    /// Limiar default: `θ = 0,90`, `n_min = 5`, `z = 1,645`.
    ///
    /// Com registo perfeito o limite inferior de Wilson cruza `0,90` a **n = 25** ensaios (LB
    /// `902 ≥ 900`); a `n = 24` fica em `899`. É esse o custo de provar `Enforced` — medido em
    /// `bench/e18/confidence/`.
    pub const DEFAULT: Self = Self {
        theta_milli: 900,
        n_min: 5,
        z_milli: 1_645,
    };
}

/// Confiança de uma regra declarada `Enforced` (vocabulário **fechado**).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Confidence {
    /// Provada: `n ≥ n_min` **e** `LB ≥ θ`.
    Enforced,
    /// Demovida a `Advisory`: há evidência suficiente e ela **não** sustenta a categoria.
    Advisory,
    /// Sem observações: a categoria é declarada, não medida.
    Unmeasured,
}

impl Confidence {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Advisory => "advisory",
            Self::Unmeasured => "unmeasured",
        }
    }
}

/// Veredicto de confiança de uma regra, com a evidência que o sustenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Regra.
    pub id: RuleId,
    /// Confiança medida.
    pub confidence: Confidence,
    /// Ensaios observados.
    pub trials: u32,
    /// Sucessos observados.
    pub successes: u32,
    /// Limite inferior de Wilson em milésimos.
    pub lower_milli: u32,
    /// `true` se há evidência suficiente **contra** a categoria declarada: `n ≥ n_min` e `LB < θ`
    /// com pelo menos uma falha. É a condição que uma borda trata como falha **medida**; *não
    /// provado* (poucas observações) não é contradição.
    pub contradiction: bool,
    /// Motivo legível, **com a evidência** (é o que o humano lê).
    pub reason: Reason,
}

impl Verdict {
    /// `true` se a regra foi **provada** `Enforced`.
    #[must_use]
    pub const fn is_proven(&self) -> bool {
        matches!(self.confidence, Confidence::Enforced)
    }
}

/// Verifica uma regra `Enforced` contra os seus ensaios (pura, determinística).
#[must_use]
pub fn verdict(id: &RuleId, trials: Trials, threshold: &Threshold) -> Verdict {
    let lower_milli = trials.wilson_lower_milli(threshold.z_milli);
    let confidence = if trials.trials() == 0 {
        Confidence::Unmeasured
    } else if trials.trials() >= threshold.n_min && lower_milli >= threshold.theta_milli {
        Confidence::Enforced
    } else {
        Confidence::Advisory
    };
    let reason = Reason::new(match confidence {
        Confidence::Enforced => format!(
            "LB {lower_milli} ≥ θ {} com n = {} ({} sucessos)",
            threshold.theta_milli,
            trials.trials(),
            trials.successes()
        ),
        Confidence::Advisory if trials.trials() < threshold.n_min => format!(
            "n = {} < n_min = {}: ainda não provado (LB {lower_milli}, θ {})",
            trials.trials(),
            threshold.n_min,
            threshold.theta_milli
        ),
        Confidence::Advisory => format!(
            "LB {lower_milli} < θ {} com n = {} ({} falhas em {} ensaios): demovida a Advisory",
            threshold.theta_milli,
            trials.trials(),
            trials.trials().saturating_sub(trials.successes()),
            trials.trials()
        ),
        Confidence::Unmeasured => "sem observações: `Enforced` é declarada, não medida".to_string(),
    });
    let contradiction = confidence == Confidence::Advisory
        && trials.trials() >= threshold.n_min
        && trials.successes() < trials.trials();
    Verdict {
        id: id.clone(),
        confidence,
        trials: trials.trials(),
        successes: trials.successes(),
        lower_milli,
        contradiction,
        reason,
    }
}

/// Número de baldes do diagrama de fiabilidade (largura fixa: `1 000 / N` milésimos).
pub const CALIBRATION_BINS: u32 = 10;

/// Balde do diagrama de fiabilidade (C3/W8-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrationBin {
    /// Limite inferior do balde (milésimos).
    pub lower_milli: u32,
    /// Limite superior do balde (milésimos; exclusivo).
    pub upper_milli: u32,
    /// Ensaios no balde.
    pub trials: u32,
    /// Probabilidade prevista média (milésimos): `Σ LB·n / Σ n`.
    pub predicted_milli: u32,
    /// Frequência empírica (milésimos): `Σ sucessos / Σ n`.
    pub observed_milli: u32,
}

/// Calibração do veredicto face ao log (C3/W8-2): ECE, Brier e diagrama de fiabilidade.
///
/// A confiança **prevista** é o limite inferior de Wilson (`lower_milli`) — o número que o
/// projeto publica; o **desfecho** é a frequência empírica (`sucessos/ensaios`). É uma medida
/// *in-sample* do próprio log (não uma validação fora da amostra): mede o **conservadorismo** do
/// limite, não o acerto do modelo. Base tipada: `inferred`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Calibration {
    /// Ensaios calibrados (`Σ trials`; `0` ⇒ sem dados).
    pub trials: u32,
    /// Brier score em milésimos (`0` perfeito, `1 000` pior).
    pub brier_milli: u32,
    /// Expected Calibration Error em milésimos (`0` perfeito).
    pub ece_milli: u32,
    /// Diagrama de fiabilidade (`CALIBRATION_BINS` baldes; vazio sem dados).
    pub bins: Vec<CalibrationBin>,
}

/// Acumulador de um balde durante a calibração.
#[derive(Clone, Copy, Default)]
struct BinAccumulator {
    predicted_sum: f64,
    successes: u32,
    trials: u32,
}

/// Índice do balde de uma confiança (largura fixa; satura no último).
fn bin_index(lower_milli: u32, width: u32) -> usize {
    let divisor = 1_000_u32.saturating_div(width).max(1);
    usize::try_from(
        lower_milli
            .checked_div(divisor)
            .unwrap_or(0)
            .min(width.saturating_sub(1)),
    )
    .unwrap_or(0)
}

/// Calibra os veredictos face aos ensaios que os sustentam (pura, determinística).
///
/// Veredictos sem observações (`trials = 0`) são ignorados: nada a calibrar. A ordem de entrada
/// não muda o resultado (os baldes agregam).
#[must_use]
pub fn calibrate(verdicts: &[Verdict]) -> Calibration {
    let width = CALIBRATION_BINS.max(1);
    let mut bins = vec![BinAccumulator::default(); usize::try_from(width).unwrap_or(1)];
    let mut total = 0_u32;
    let mut brier = 0.0_f64;
    for verdict in verdicts {
        if verdict.trials == 0 {
            continue;
        }
        let n = verdict.trials;
        let p = f64::from(verdict.lower_milli) / 1_000.0;
        let miss = 1.0 - p;
        total = total.saturating_add(n);
        brier = f64::from(verdict.successes).mul_add(miss * miss, brier);
        brier = f64::from(n.saturating_sub(verdict.successes)).mul_add(p * p, brier);
        if let Some(entry) = bins.get_mut(bin_index(verdict.lower_milli, width)) {
            entry.predicted_sum =
                f64::from(verdict.lower_milli).mul_add(f64::from(n), entry.predicted_sum);
            entry.successes = entry.successes.saturating_add(verdict.successes);
            entry.trials = entry.trials.saturating_add(n);
        }
    }
    if total == 0 {
        return Calibration::default();
    }
    let total_f = f64::from(total);
    let mut ece = 0.0_f64;
    let mut out = Vec::with_capacity(bins.len());
    for (index, accumulator) in bins.into_iter().enumerate() {
        let lower = u32::try_from(index).unwrap_or(0).saturating_mul(width);
        let upper = lower.saturating_add(width);
        let (predicted_milli, observed_milli) = if accumulator.trials == 0 {
            (0, 0)
        } else {
            let predicted = accumulator.predicted_sum / f64::from(accumulator.trials) / 1_000.0;
            let observed = f64::from(accumulator.successes) / f64::from(accumulator.trials);
            ece = (f64::from(accumulator.trials) / total_f)
                .mul_add((predicted - observed).abs(), ece);
            (to_milli(predicted), to_milli(observed))
        };
        out.push(CalibrationBin {
            lower_milli: lower,
            upper_milli: upper,
            trials: accumulator.trials,
            predicted_milli,
            observed_milli,
        });
    }
    Calibration {
        trials: total,
        brier_milli: to_milli(brier / total_f),
        ece_milli: to_milli(ece),
        bins: out,
    }
}

#[cfg(test)]
mod tests;
