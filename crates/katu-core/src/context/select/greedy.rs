//! Escolha greedy: utilidade submodular + diversidade MMR + fusão RRF sob orçamento.
//!
//! O greedy maximiza uma função **submodular** (massa de informação de termos ainda não cobertos),
//! pelo que a garantia `1 − 1/e` de Nemhauser et al. se aplica: nenhuma heurística simples o bate de
//! forma garantida. O custo é O(n²·termos) — por isso o `idf` é pré-calculado, o ganho marginal não
//! aloca conjuntos temporários e a pertença ao escolhido é um bitmap.

use std::collections::{BTreeMap, BTreeSet};

use super::{CHANNELS, Candidate, SelectionParams};
use crate::diag::{Level, events};
use crate::evidence::from_f64;

/// Estatística de termos do conjunto (IDF determinístico, sem corpus externo).
///
/// O `idf` é **pré-calculado** em [`Stats::of`]: a consulta passa a ser uma leitura de mapa, o que
/// importa porque o greedy consulta a massa O(n²) vezes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    /// `idf` por termo, em milésimos de nat.
    idf: BTreeMap<String, u64>,
    /// Número de candidatos.
    n: u32,
}

impl Stats {
    /// Calcula `df`/`n` e o `idf` de cada termo a partir dos candidatos.
    #[must_use]
    pub fn of(candidates: &[Candidate]) -> Self {
        let _span = crate::trace_fn!("context::select::stats");

        let mut df: BTreeMap<&str, u32> = BTreeMap::new();
        for candidate in candidates {
            for term in &candidate.terms {
                let counter = df.entry(term.as_str()).or_insert(0);
                *counter = counter.saturating_add(1);
            }
        }
        let n = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        let mut idf: BTreeMap<String, u64> = BTreeMap::new();
        for (term, count) in df {
            idf.insert(term.to_string(), idf_milli(n, count));
        }
        Self { idf, n }
    }

    /// `idf` de um termo em milésimos de nat; termo ausente ⇒ peso máximo (não é redundante).
    #[must_use]
    pub fn idf_milli(&self, term: &str) -> u64 {
        let _span = crate::trace_fn!("context::select::idf_milli");

        self.idf
            .get(term)
            .copied()
            .unwrap_or_else(|| idf_milli(self.n, 0))
    }

    /// Massa de informação de um conjunto de termos (milésimos de nat).
    #[must_use]
    pub fn mass_milli(&self, terms: &BTreeSet<String>) -> u64 {
        let _span = crate::fn_span!(
            Level::Trace,
            events::CONTEXT_TRIM,
            "context::select::mass_milli",
            "terms" => terms.len(),
        );

        terms
            .iter()
            .map(|term| self.idf_milli(term))
            .fold(0u64, u64::saturating_add)
    }

    /// Massa dos termos de `terms` que **não** estão em `covered` (ganho marginal).
    ///
    /// Sem conjunto temporário: o ganho marginal é consultado O(n²) vezes e alocar um `BTreeSet`
    /// por consulta dominaria o custo da seleção.
    #[must_use]
    pub fn marginal_milli(&self, terms: &BTreeSet<String>, covered: &BTreeSet<String>) -> u64 {
        let _span = crate::trace_fn!("context::select::marginal_milli");

        terms
            .iter()
            .filter(|term| !covered.contains(*term))
            .map(|term| self.idf_milli(term))
            .fold(0u64, u64::saturating_add)
    }
}

/// `idf(t) = ln(1 + n/df(t))` em milésimos de nat. `df = 0` (termo ausente) ⇒ peso máximo.
fn idf_milli(n: u32, df: u32) -> u64 {
    let _span = crate::trace_fn!("context::select::idf_of");

    let n = f64::from(n);
    let df = f64::from(df);
    let ratio = if df <= 0.0 { n } else { n / df };
    let value = ratio.ln_1p() * 1_000.0;
    if value.is_finite() && value > 0.0 {
        from_f64(value)
    } else {
        0
    }
}

/// Escolha greedy submodular com MMR e fusão RRF, sob orçamento de tokens.
///
/// `pinned` é a única unidade obrigatória (o turno corrente, que fica **intacto**): se nem ela
/// cabe, a seleção devolve só `pinned` — o chamador decide recuar para a política de sufixo.
/// Devolve índices de candidatos, em ordem crescente.
#[must_use]
pub fn greedy(
    candidates: &[Candidate],
    budget: usize,
    pinned: Option<usize>,
    goal: &BTreeSet<String>,
    params: SelectionParams,
) -> Vec<usize> {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_TRIM,
        "context::select::greedy",
        "candidates" => candidates.len(),
        "budget" => budget,
    );

    let stats = Stats::of(candidates);
    let rrf = rrf_scores(candidates, &stats, goal, params);
    let mut pick = Pick::new(candidates.len());
    if !seed(candidates, pinned, budget, &mut pick) {
        return pick.chosen;
    }
    let lambda = u64::from(params.lambda_milli.min(1_000));
    loop {
        let mut best: Option<(u64, usize)> = None;
        for (index, candidate) in candidates.iter().enumerate() {
            if pick.taken.get(index).copied().unwrap_or(true) {
                continue;
            }
            if pick.used.saturating_add(candidate.tokens) > budget {
                continue;
            }
            let similarity = max_similarity(candidate, candidates, &pick.taken);
            if similarity > u64::from(params.sim_max_milli) {
                continue;
            }
            let marginal = stats.marginal_milli(&candidate.terms, &pick.covered);
            let utility = marginal.saturating_mul(1_000u64.saturating_sub(lambda));
            let penalty = similarity.saturating_mul(lambda);
            let score = utility
                .saturating_add(rrf.get(index).copied().unwrap_or(0))
                .saturating_sub(penalty);
            if best.is_none_or(|(best_score, best_index)| {
                score > best_score || (score == best_score && index < best_index)
            }) {
                best = Some((score, index));
            }
        }
        let Some((_, index)) = best else {
            break;
        };
        pick.take(candidates, index);
    }
    pick.chosen.sort_unstable();
    pick.chosen
}

/// Estado acumulado da escolha greedy (evita seis parâmetros a passear pela recursão lógica).
struct Pick {
    /// Bitmap de candidatos já escolhidos (O(1) por consulta, sem varrer a lista).
    taken: Vec<bool>,
    /// Índices escolhidos.
    chosen: Vec<usize>,
    /// Termos já cobertos (o ganho marginal só conta o que falta).
    covered: BTreeSet<String>,
    /// Orçamento já usado, em tokens.
    used: usize,
}

impl Pick {
    /// Estado vazio para `n` candidatos.
    fn new(n: usize) -> Self {
        let _span = crate::trace_fn!("context::select::greedy::Pick::new");

        Self {
            taken: vec![false; n],
            chosen: Vec::new(),
            covered: BTreeSet::new(),
            used: 0,
        }
    }

    /// Toma o candidato `index` (marca-o, acumula termos e custo).
    fn take(&mut self, candidates: &[Candidate], index: usize) {
        let _span = crate::trace_fn!("context::select::pick_take");

        if let Some(candidate) = candidates.get(index) {
            self.covered.extend(candidate.terms.iter().cloned());
            self.used = self.used.saturating_add(candidate.tokens);
        }
        if let Some(slot) = self.taken.get_mut(index) {
            *slot = true;
        }
        self.chosen.push(index);
    }
}

/// Semeia a escolha com a unidade **obrigatória** (o turno corrente).
///
/// Devolve `false` se a unidade obrigatória não cabe no orçamento — caso em que o chamador recua
/// para o sufixo (nunca para um contexto inválido). Sem unidade obrigatória, devolve `true`.
fn seed(candidates: &[Candidate], pinned: Option<usize>, budget: usize, pick: &mut Pick) -> bool {
    let _span = crate::trace_fn!("context::select::seed");

    let Some((index, candidate)) =
        pinned.and_then(|index| candidates.get(index).map(|c| (index, c)))
    else {
        return true;
    };
    if candidate.tokens > budget {
        return false;
    }
    pick.take(candidates, index);
    true
}

/// Similaridade máxima (Jaccard, milésimos) entre um candidato e os já escolhidos.
fn max_similarity(candidate: &Candidate, candidates: &[Candidate], taken: &[bool]) -> u64 {
    let _span = crate::trace_fn!("context::select::max_similarity");

    let mut worst = 0u64;
    for (index, other) in candidates.iter().enumerate() {
        if !taken.get(index).copied().unwrap_or(false) {
            continue;
        }
        worst = worst.max(jaccard_milli(&candidate.terms, &other.terms));
    }
    worst
}

/// Jaccard em milésimos (`0..=1000`) numa só passagem; conjuntos vazios valem `0`.
///
/// A travessia é feita por cursores sobre dois `Vec` de *slices* (barato: só ponteiros) em vez de
/// duas travessias (`union().count()` + `intersection().count()`), o que importa porque a
/// similaridade é consultada O(n²) vezes pelo greedy.
#[must_use]
pub fn jaccard_milli(left: &BTreeSet<String>, right: &BTreeSet<String>) -> u64 {
    let _span = crate::trace_fn!("context::select::jaccard_milli");

    if left.is_empty() || right.is_empty() {
        return 0;
    }
    let left: Vec<&str> = left.iter().map(String::as_str).collect();
    let right: Vec<&str> = right.iter().map(String::as_str).collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut intersection, mut union) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        let (a, b) = (left.get(i), right.get(j));
        match (a, b) {
            (Some(a), Some(b)) => match a.cmp(b) {
                std::cmp::Ordering::Less => i = i.saturating_add(1),
                std::cmp::Ordering::Greater => j = j.saturating_add(1),
                std::cmp::Ordering::Equal => {
                    i = i.saturating_add(1);
                    j = j.saturating_add(1);
                    intersection = intersection.saturating_add(1);
                }
            },
            _ => break,
        }
        union = union.saturating_add(1);
    }
    union = union.saturating_add(left.len().saturating_sub(i));
    union = union.saturating_add(right.len().saturating_sub(j));
    if union == 0 {
        return 0;
    }
    let scaled = u64::try_from(intersection)
        .unwrap_or(u64::MAX)
        .saturating_mul(1_000);
    scaled
        .checked_div(u64::try_from(union).unwrap_or(1))
        .unwrap_or(0)
}

/// Fusão **RRF** dos canais: `score(i) = Σ_c w_c / (k + rank_c(i))`.
///
/// Os canais são rankings independentes sobre os mesmos candidatos; o RRF combina-os sem calibrar
/// escalas (o que um `Σ w·score` exigiria). Empate de score → ordem do índice (determinismo).
fn rrf_scores(
    candidates: &[Candidate],
    stats: &Stats,
    goal: &BTreeSet<String>,
    params: SelectionParams,
) -> Vec<u64> {
    let _span = crate::trace_fn!("context::select::rrf_scores");

    let n = candidates.len();
    let mut rankings: Vec<Vec<usize>> = Vec::with_capacity(CHANNELS.len());
    // Canal `recencia`: o mais recente primeiro.
    rankings.push((0..n).rev().collect());
    // Canal `massa`: massa total de informação, decrescente.
    rankings.push(rank_by(candidates, |_, candidate| {
        stats.mass_milli(&candidate.terms)
    }));
    // Canal `objetivo`: sobreposição com os termos do objetivo, decrescente.
    rankings.push(rank_by(candidates, |_, candidate| {
        u64::try_from(candidate.terms.intersection(goal).count()).unwrap_or(u64::MAX)
    }));
    // Canal `evidencia`: o que traz prova (resultado de tool) primeiro.
    rankings.push(rank_by(candidates, |_, candidate| candidate.evidence));

    let k = u64::from(params.k_rrf);
    let mut scores = vec![0u64; n];
    for (channel, ranking) in rankings.iter().enumerate() {
        let weight = u64::from(params.channel_weights.get(channel).copied().unwrap_or(0));
        for (rank, index) in ranking.iter().enumerate() {
            let rank = u64::try_from(rank).unwrap_or(u64::MAX);
            let share = weight
                .saturating_mul(1_000)
                .checked_div(k.saturating_add(rank).saturating_add(1))
                .unwrap_or(0);
            if let Some(slot) = scores.get_mut(*index) {
                *slot = slot.saturating_add(share);
            }
        }
    }
    scores
}

/// Ordena índices por um valor decrescente; empate pelo índice crescente.
///
/// O valor é calculado **uma vez** por candidato (e não a cada comparação) e a ordenação é
/// instável — o desempate explícito garante o determinismo sem pagar a estabilidade.
fn rank_by(candidates: &[Candidate], value: impl Fn(usize, &Candidate) -> u64) -> Vec<usize> {
    let _span = crate::trace_fn!("context::select::rank_by");

    let mut keyed: Vec<(u64, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (value(index, candidate), index))
        .collect();
    keyed.sort_unstable_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    keyed.into_iter().map(|(_, index)| index).collect()
}
