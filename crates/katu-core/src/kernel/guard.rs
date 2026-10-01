//! Deteção de **loop patológico** por artefacto (Q-12/F7): CUSUM + e-value + novidade por
//! assinatura.
//!
//! O único travão até aqui era o teto global de passos (`AgentError::TooManySteps`), que corta
//! **depois** de o orçamento ter ardido devagar (brecha (h) do `OPTIMIZATION_PLAN.md` §1.4). Aqui
//! vive o sinal cedo: cada passo do turno deixa uma **assinatura** (nome da tool + argumentos
//! canónicos, hash FNV-1a determinístico) e duas estatísticas de deteção de mudança:
//!
//! - **CUSUM** sobre a *fração de repetição* do passo (`1 − novidade`, contínua): alarme quando a
//!   média se desvia de forma **sustentada** (`S ← max(0, S + x − k)`, alarme se `S ≥ h`).
//! - **e-value** sobre o binário "o passo inteiro já foi visto" (novidade nula): a razão de
//!   verosimilhança `Λ_n` entre `H1: loop (p₁)` e `H0: normal (p₀)` é um **martingale não-negativo**
//!   sob `H0`; rejeita-se quando `log Λ_n ≥ log(1/α)`. Por **Ville**, `P_{H0}(∃n: Λ_n ≥ 1/α) ≤ α`
//!   para **qualquer** regra de parada — a rejeição é *anytime-valid* (parada opcional), ao
//!   contrário da fronteira `log((1−β)/α)` do SPRT clássico, que só garante `α` a `n` fixo
//!   (C1/W8-3; a fronteira nominal continua disponível como sinal, não como corte).
//!
//! **Progresso reinicia.** Um passo com uma chamada de **escrita** (não `Shared`, isto é, exclusiva:
//! `write`/`edit`/`bash`/...) reinicia o CUSUM, o SPRT e a memória de assinaturas: fazer progresso
//! não é repetir. É esta a razão por que um *polling* legítimo (`bash "make"` em ciclo) **não** é
//! cortado — só o ciclo de leitura sem progresso é.
//!
//! O alarme **nunca** é silencioso: o chamador corta o turno com um erro que nomeia o que se repetiu,
//! emite [`crate::diag::events::AGENT_LOOP`] e fecha o turno.

use std::collections::BTreeSet;

use serde_json::Value;

/// Impressão determinística de uma chamada (nome + argumentos canónicos).
///
/// `serde_json` serializa as chaves de um objeto por ordem canónica, pelo que a impressão não
/// depende da ordem de inserção; o hash é FNV-1a de 64 bits (sem RNG, estável entre execuções).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fingerprint(u64);

/// FNV-1a de 64 bits sobre bytes (delegado no [`super::hash`] canónico: um só sítio).
fn fnv1a(bytes: &[u8]) -> u64 {
    let _span = crate::trace_fn!("kernel::guard::fnv1a");

    super::hash::fnv1a(bytes)
}

impl Fingerprint {
    /// Impressão de uma chamada: `nome` + `\x1f` + JSON canónico dos argumentos.
    #[must_use]
    pub fn of(name: &str, arguments: &Value) -> Self {
        let _span = crate::trace_fn!("kernel::guard::of");

        let mut text = String::with_capacity(name.len().saturating_add(32));
        text.push_str(name);
        text.push('\u{1f}');
        text.push_str(&arguments.to_string());
        Self(fnv1a(text.as_bytes()))
    }

    /// Bits da impressão (para telemetria e testes).
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// Chamada observada: impressão + se **altera** o workspace.
///
/// `exclusive` é o que separa um ciclo patológico de um progresso: `write`/`edit`/`move`/`bash`
/// alteram o mundo e **reiniciam** o detector; `read`/`grep`/`find`/`ls` não.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call {
    /// Impressão determinística.
    pub print: Fingerprint,
    /// `true` se a chamada altera o workspace (concorrência exclusiva).
    pub mutates: bool,
}

impl Call {
    /// Chamada que **não** altera o workspace (só-leitura).
    #[must_use]
    pub const fn shared(print: Fingerprint) -> Self {
        Self {
            print,
            mutates: false,
        }
    }

    /// Chamada que **altera** o workspace (exclusiva).
    #[must_use]
    pub const fn exclusive(print: Fingerprint) -> Self {
        Self {
            print,
            mutates: true,
        }
    }
}

/// Parâmetros do detector (dados, não derivados da observação — DF8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardParams {
    /// Alvo do CUSUM: fração de repetição tolerada por passo (milésimos).
    pub cusum_k_milli: u32,
    /// Limiar de alarme do CUSUM (milésimos).
    pub cusum_h_milli: u32,
    /// `H0` (turno normal): probabilidade de um passo inteiramente repetido (milésimos).
    pub e_value_p0_milli: u32,
    /// `H1` (loop): probabilidade de um passo inteiramente repetido (milésimos).
    pub e_value_p1_milli: u32,
    /// Falso positivo alvo **anytime-valid** da e-value (milésimos).
    pub e_value_alpha_milli: u32,
    /// Passos mínimos antes de poder alarmar (não cortar um turno curto).
    pub min_steps: u32,
}

impl GuardParams {
    /// Defaults conservadores: `k = 0,5`, `h = 2,0`, `p₀ = 0,1`, `p₁ = 0,6`, `α = 0,01`,
    /// `min_steps = 3`.
    ///
    /// Consequências (medidas em `bench/e18/loop/`): com o passo inteiramente repetido, a e-value
    /// alarme no **5.º** passo (o 1.º é novidade e contribui contra) e o CUSUM precisaria de **5**
    /// repetições seguidas. Um turno sem repetições nunca alarme (`S` fica em 0 e `Λ` desce).
    pub const DEFAULT: Self = Self {
        cusum_k_milli: 500,
        cusum_h_milli: 2_000,
        e_value_p0_milli: 100,
        e_value_p1_milli: 600,
        e_value_alpha_milli: 10,
        min_steps: 3,
    };

    /// Fronteira de rejeição **anytime-valid**: `log(1/α)` em milésimos.
    ///
    /// É o limiar de Ville, mais conservador do que o `log((1−β)/α)` do SPRT: paga um passo a
    /// mais no alarme em troca da garantia de cobertura a qualquer `n`.
    #[must_use]
    pub fn e_value_threshold_milli(&self) -> i64 {
        let _span = crate::trace_fn!("kernel::guard::e_value_threshold_milli");

        log_ratio(1_000, self.e_value_alpha_milli.max(1))
    }
}

/// Qual detector disparou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlarmKind {
    /// Desvio sustentado da fração de repetição (CUSUM).
    Cusum,
    /// Rejeição anytime-valid da razão de verosimilhança (e-value).
    EValue,
}

impl AlarmKind {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cusum => "cusum",
            Self::EValue => "evalue",
        }
    }
}

/// Alarme com a evidência que o sustenta (o que o humano lê).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alarm {
    /// Detector.
    pub kind: AlarmKind,
    /// Passo em que disparou (1-based).
    pub step: u32,
    /// Passos inteiramente repetidos consecutivos.
    pub repeated: u32,
    /// Novidade do passo em milésimos (1 = tudo novo).
    pub novelty_milli: u32,
    /// Estatística do CUSUM em milésimos.
    pub cusum_milli: u32,
    /// Log da e-value em milésimos (`log Λ`).
    pub e_value_log_milli: i64,
    /// Assinatura do passo, em hexadecimal (o que se repetiu).
    pub signature: u64,
}

impl Alarm {
    /// Motivo legível, com a evidência.
    #[must_use]
    pub fn reason(&self) -> String {
        let _span = crate::trace_fn!("kernel::guard::reason");

        format!(
            "loop detectado ({}) no passo {}: {} passo(s) repetido(s), novidade {}‰, CUSUM {}‰, e-value {}‰",
            self.kind.as_str(),
            self.step,
            self.repeated,
            self.novelty_milli,
            self.cusum_milli,
            self.e_value_log_milli
        )
    }
}

/// Detector de loop de um turno (estado mutável, determinístico).
#[derive(Debug, Clone)]
pub struct Guard {
    params: GuardParams,
    seen: BTreeSet<u64>,
    cusum_milli: i64,
    e_value_log_milli: i64,
    steps: u32,
    repeated: u32,
}

impl Guard {
    /// Cria o detector com os parâmetros dados.
    #[must_use]
    pub fn new(params: GuardParams) -> Self {
        let _span = crate::trace_fn!("kernel::guard::new");

        Self {
            params,
            seen: BTreeSet::new(),
            cusum_milli: 0,
            e_value_log_milli: 0,
            steps: 0,
            repeated: 0,
        }
    }

    /// Cria o detector com os defaults.
    #[must_use]
    pub fn with_defaults() -> Self {
        let _span = crate::trace_fn!("kernel::guard::with_defaults");

        Self::new(GuardParams::DEFAULT)
    }

    /// Observa um passo e devolve um alarme, se houver.
    ///
    /// `calls` são as chamadas do passo **na ordem do modelo**; um passo com uma chamada exclusiva
    /// (que altera o workspace) é **progresso** e reinicia o detector.
    pub fn observe(&mut self, calls: &[Call]) -> Option<Alarm> {
        let _span = crate::trace_fn!("kernel::guard::observe");

        self.steps = self.steps.saturating_add(1);
        if calls.is_empty() {
            return None;
        }
        if calls.iter().any(|call| call.mutates) {
            self.seen.clear();
            self.cusum_milli = 0;
            self.e_value_log_milli = 0;
            self.repeated = 0;
        }
        let unique: BTreeSet<u64> = calls.iter().map(|call| call.print.bits()).collect();
        let fresh = unique
            .iter()
            .filter(|bits| !self.seen.contains(bits))
            .count();
        let ratio = fresh
            .saturating_mul(1_000)
            .checked_div(unique.len())
            .unwrap_or(0);
        let novelty_milli = u32::try_from(ratio).unwrap_or(1_000);
        self.seen.extend(unique.iter().copied());
        if fresh == 0 {
            self.repeated = self.repeated.saturating_add(1);
        } else {
            self.repeated = 0;
        }
        self.update_cusum(novelty_milli);
        self.update_e_value(fresh);
        if self.steps < self.params.min_steps {
            return None;
        }
        let signature = unique.iter().next().copied().unwrap_or_default();
        // A e-value (anytime-valid) tem primazia sobre o CUSUM (heurístico) no mesmo passo: o corte
        // formal é o que a estatística justifica; o CUSUM fica como sinal para a repetição parcial.
        if self.e_value_alarmed() {
            return Some(self.alarm(AlarmKind::EValue, novelty_milli, signature));
        }
        if self.cusum_milli >= i64::from(self.params.cusum_h_milli) {
            return Some(self.alarm(AlarmKind::Cusum, novelty_milli, signature));
        }
        None
    }

    /// CUSUM sobre a fração de repetição: `S ← max(0, S + x − k)`.
    fn update_cusum(&mut self, novelty_milli: u32) {
        let _span = crate::trace_fn!("kernel::guard::update_cusum");

        let repetition = 1_000_i64.saturating_sub(i64::from(novelty_milli));
        let delta = repetition.saturating_sub(i64::from(self.params.cusum_k_milli));
        self.cusum_milli = self.cusum_milli.saturating_add(delta).max(0);
    }

    /// e-value sobre o binário "passo inteiramente repetido" (`fresh == 0`).
    fn update_e_value(&mut self, fresh: usize) {
        let _span = crate::trace_fn!("kernel::guard::update_e_value");

        let p1 = self.params.e_value_p1_milli;
        let p0 = self.params.e_value_p0_milli;
        let step = if fresh == 0 {
            log_ratio(p1, p0)
        } else {
            log_ratio(1_000_u32.saturating_sub(p1), 1_000_u32.saturating_sub(p0))
        };
        self.e_value_log_milli = self.e_value_log_milli.saturating_add(step);
    }

    /// `log Λ ≥ log(1/α)`: rejeição válida a qualquer `n` (Ville).
    fn e_value_alarmed(&self) -> bool {
        let _span = crate::trace_fn!("kernel::guard::e_value_alarmed");

        self.e_value_log_milli >= self.params.e_value_threshold_milli()
    }

    /// Estatística do CUSUM (milésimos) — telemetria/testes.
    #[must_use]
    pub const fn cusum_milli(&self) -> i64 {
        self.cusum_milli
    }

    /// Log da e-value (milésimos) — telemetria/testes.
    #[must_use]
    pub const fn e_value_log_milli(&self) -> i64 {
        self.e_value_log_milli
    }

    /// Passos observados.
    #[must_use]
    pub const fn steps(&self) -> u32 {
        self.steps
    }

    /// Constrói o alarme com a evidência corrente.
    fn alarm(&self, kind: AlarmKind, novelty_milli: u32, signature: u64) -> Alarm {
        let _span = crate::trace_fn!("kernel::guard::alarm");

        Alarm {
            kind,
            step: self.steps,
            repeated: self.repeated,
            novelty_milli,
            cusum_milli: u32::try_from(self.cusum_milli.max(0)).unwrap_or(u32::MAX),
            e_value_log_milli: self.e_value_log_milli,
            signature,
        }
    }
}

/// `log(p₁/p₀)` em milésimos (determinístico).
fn log_ratio(numerator_milli: u32, denominator_milli: u32) -> i64 {
    let _span = crate::trace_fn!("kernel::guard::log_ratio");

    let ratio = f64::from(numerator_milli) / f64::from(denominator_milli.max(1));
    to_milli(ratio.ln())
}

/// Converte um `f64` (log-razão em milésimos) para inteiro, saturando.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "log-razão arredondada a milésimos; a saturação evita overflow (precedente: `evidence::from_f64`)"
)]
fn to_milli(value: f64) -> i64 {
    let _span = crate::trace_fn!("kernel::guard::to_milli");

    let scaled = value * 1_000.0;
    if scaled.is_nan() {
        return 0;
    }
    scaled.clamp(-1.0e9, 1.0e9).round() as i64
}

#[cfg(test)]
mod tests;
