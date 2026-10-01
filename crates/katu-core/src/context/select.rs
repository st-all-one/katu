//! Seleção de contexto por **informação** (Q-02b/Q-03): determinística, sem modelo e sem corpus.
//!
//! Duas decisões partilham a **mesma** máquina — é isso que S-01 exige (um só caminho de orçamento,
//! não dois mecanismos a competir pelo mesmo teto):
//!
//! - **Q-02b** — que *unidades* do histórico cabem no orçamento cru (`raw_min`): em vez do sufixo
//!   mais recente, escolhe-se o que **informa** (massa IDF sobre termos novos, submodular), com
//!   diversidade **MMR** e fusão de canais **RRF** (recência, massa, objetivo, evidência).
//! - **Q-03** — que *linhas* do prefixo entram no digest (`summary_max`): mesma utilidade, mesmo
//!   teto, mesma fusão.
//!
//! A utilidade é `I(S) = Σ_{t ∈ S} idf(t)` com `idf(t) = ln(1 + n/df(t))` — os `df` vêm do próprio
//! conjunto (nenhum corpus externo, nenhum tokenizer), pelo que o resultado é reprodutível. O ganho
//! marginal de um candidato é a massa dos termos **ainda não cobertos**: a função é submodular e o
//! greedy tem a garantia `1 − 1/e` (Nemhauser et al.), pelo que não se faz melhor com força bruta.
//!
//! **Custo (`.agents/skill/rust`):** o caminho é quente (corre a cada turno, sobre todo o log), pelo
//! que o texto model-visible sai como [`Cow`] (nada se clona só para tokenizar), o `idf` é calculado
//! **uma vez** por conjunto e não a cada consulta, o ganho marginal não constrói conjuntos temporários
//! e a pertença ao escolhido é um bitmap (O(1)) em vez de uma procura linear na lista.
//!
//! Nada aqui decide sozinho: as políticas `Suffix` (histórica) e `Utility` (Q-02b) coexistem, e a
//! adoção da segunda depende de A/B publicado (`bench/e18/select/`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use crate::kernel::Message;

mod greedy;
mod info;
mod units;

pub use greedy::{Stats, greedy, jaccard_milli};
pub use info::{js_milli, term_counts};
pub use units::{message_text, units};

#[cfg(test)]
mod tests;

/// Versão do esquema de seleção. Mudar canais, pesos ou o algoritmo exige incrementar isto.
pub const SELECTION_SCHEMA_VERSION: u32 = 1;

/// Mínimo de caracteres de um termo (abaixo disto é ruído de tokenização).
const MIN_TERM_CHARS: usize = 3;

/// Canais fundidos por RRF, por ordem estável (o índice é o da tabela de pesos).
pub const CHANNELS: [&str; 4] = ["recencia", "massa", "objetivo", "evidencia"];

/// Parâmetros da seleção — **dados** versionados, não constantes escondidas no algoritmo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionParams {
    /// Peso da penalização de diversidade (MMR), em milésimos de `I`. `0` = só utilidade.
    pub lambda_milli: u32,
    /// Teto de similaridade de Jaccard (milésimos) entre candidatos escolhidos.
    pub sim_max_milli: u32,
    /// Constante da fusão RRF (`score = Σ w/(k + rank)`); `60` é o valor canónico.
    pub k_rrf: u32,
    /// Gatilho de compactação: `JS(prefixo ‖ sufixo)` mínimo (milésimos de nat) para valer a pena.
    pub tau_js_milli: u32,
    /// Pesos dos canais, na ordem de [`CHANNELS`].
    pub channel_weights: [u32; CHANNELS.len()],
}

impl SelectionParams {
    /// Valores por omissão (`const` para poderem viver em `AssembleOptions::new`).
    pub const DEFAULT: Self = Self {
        lambda_milli: 250,
        sim_max_milli: 700,
        k_rrf: 60,
        tau_js_milli: 200,
        channel_weights: [1_000, 1_000, 1_000, 500],
    };
}

impl Default for SelectionParams {
    fn default() -> Self {
        let _span = crate::trace_fn!("context::select::default_params");

        Self::DEFAULT
    }
}

/// Política de seleção sob orçamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionPolicy {
    /// Sufixo mais recente (comportamento histórico; default — nada muda sem A/B).
    #[default]
    Suffix,
    /// Utilidade submodular + MMR + RRF (Q-02b/Q-03), medida em `bench/e18/select/`.
    Utility,
}

impl SelectionPolicy {
    /// Nome estável (diagnóstico/artefacto/config).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suffix => "suffix",
            Self::Utility => "utility",
        }
    }

    /// Lê a política de um valor de config (`suffix`/`utility`); desconhecido ⇒ `None` (o chamador
    /// aplica o default e **não** inventa).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let _span = crate::trace_fn!("context::select::parse_policy");

        match raw {
            "suffix" => Some(Self::Suffix),
            "utility" => Some(Self::Utility),
            _ => None,
        }
    }
}

/// Candidato à seleção: os termos que traz, o que custa e quanta prova carrega.
///
/// `bytes` é o custo **exato** em bytes e `tokens` o custo orçamentado. Guardam-se os dois porque a
/// política cronológica compara bytes acumulados (como sempre fez, byte a byte) e a de utilidade
/// compara tokens; derivar um do outro com `ceil` não reproduziria a soma histórica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Termos distintos do candidato.
    pub terms: BTreeSet<String>,
    /// Custo exato em bytes.
    pub bytes: usize,
    /// Custo orçamentado em tokens.
    pub tokens: usize,
    /// Prioridade de evidência (`0` = nenhuma): canal `evidencia` da fusão RRF.
    pub evidence: u64,
}

impl Candidate {
    /// Candidato a partir do texto que traz e da prioridade de evidência.
    #[must_use]
    pub fn of(text: &str, evidence: u64) -> Self {
        let _span = crate::trace_fn!("context::select::candidate_of");

        let bytes = text.len();
        Self {
            terms: terms(text),
            bytes,
            tokens: super::tokens_from_bytes(bytes),
            evidence,
        }
    }
}

/// Unidade de contexto: mensagem isolada ou **corrida maximal** de mensagens de tool.
///
/// A fronteira é a mesma do corte (Q-02a): o wire exige que um `role: "tool"` responda ao
/// `tool_calls` precedente, pelo que um pedido e o seu resultado nunca se separam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// Índice da primeira mensagem (inclusive).
    pub start: usize,
    /// Índice da última mensagem (exclusivo).
    pub end: usize,
    /// Candidato correspondente (termos e custo).
    pub candidate: Candidate,
}

/// Termos de um texto: minúsculas, alfanuméricos, com `MIN_TERM_CHARS` caracteres ou mais.
///
/// Sem lista de *stopwords*: a redundância é captada pelo próprio `idf` (uma palavra comum tem `df`
/// alto e peso baixo), o que evita mais um dado a manter. A contagem de caracteres só corre depois
/// do teste barato em bytes — a maioria dos tokens é rejeitada sem varrer nada.
#[must_use]
pub fn terms(text: &str) -> BTreeSet<String> {
    let _span = crate::trace_fn!("context::select::terms");

    let mut out = BTreeSet::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else {
            push_term(&mut current, &mut out);
        }
    }
    push_term(&mut current, &mut out);
    out
}

/// Fecha o termo em construção: guarda-o se for longo o suficiente e reinicia o acumulador.
fn push_term(current: &mut String, out: &mut BTreeSet<String>) {
    let _span = crate::trace_fn!("context::select::push_term");

    if current.len() >= MIN_TERM_CHARS && current.chars().count() >= MIN_TERM_CHARS {
        out.insert(std::mem::take(current));
    } else {
        current.clear();
    }
}

/// Índice da primeira unidade que entra no **sufixo mais recente** que cabe em `budget`.
///
/// Réplica exata da política histórica (`fit_raw`), incluindo o caso das unidades de custo zero.
#[must_use]
pub fn suffix_start(units: &[Unit], budget: usize) -> usize {
    let _span = crate::trace_fn!("context::select::suffix_start");

    let mut used = 0usize;
    let mut index = units.len();
    while index > 0 {
        let Some(unit) = units.get(index.saturating_sub(1)) else {
            break;
        };
        if used.saturating_add(unit.candidate.tokens) > budget {
            break;
        }
        used = used.saturating_add(unit.candidate.tokens);
        index = index.saturating_sub(1);
    }
    index
}

/// Unidades escolhidas sob `policy`, em ordem crescente de índice.
///
/// `Suffix` é a política histórica (sufixo contíguo mais recente). `Utility` (Q-02b) fixa a
/// **última** unidade — o turno corrente fica intacto — e escolhe as restantes por utilidade; se nem
/// a última cabe, recua para o sufixo (que também devolve vazio), nunca para um contexto inválido.
#[must_use]
pub fn chosen_units(
    units: &[Unit],
    budget: usize,
    policy: SelectionPolicy,
    goal: &str,
    params: SelectionParams,
) -> Vec<usize> {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_TRIM,
        "context::select::chosen_units",
        "units" => units.len(),
        "budget" => budget,
        "policy" => policy.as_str(),
    );

    let start = suffix_start(units, budget);
    match policy {
        SelectionPolicy::Suffix => (start..units.len()).collect(),
        SelectionPolicy::Utility => {
            let candidates: Vec<Candidate> =
                units.iter().map(|unit| unit.candidate.clone()).collect();
            let chosen = greedy(
                &candidates,
                budget,
                units.len().checked_sub(1),
                &terms(goal),
                params,
            );
            if chosen.is_empty() {
                (start..units.len()).collect()
            } else {
                chosen
            }
        }
    }
}

/// Mensagens das unidades escolhidas, na **ordem do log** (a causalidade do modelo).
#[must_use]
pub fn kept_messages(messages: &[Message], units: &[Unit], chosen: &[usize]) -> Vec<Message> {
    let _span = crate::trace_fn!("context::select::kept_messages");

    let mut out = Vec::new();
    for index in chosen {
        let Some(unit) = units.get(*index) else {
            continue;
        };
        out.extend(
            messages
                .get(unit.start..unit.end)
                .unwrap_or_default()
                .iter()
                .cloned(),
        );
    }
    out
}

/// Mensagens das unidades **não** escolhidas (o prefixo que o digest resume).
#[must_use]
pub fn dropped_messages(messages: &[Message], units: &[Unit], chosen: &[usize]) -> Vec<Message> {
    let _span = crate::trace_fn!("context::select::dropped_messages");

    let mut keep = vec![false; units.len()];
    for index in chosen {
        if let Some(slot) = keep.get_mut(*index) {
            *slot = true;
        }
    }
    let mut out = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if keep.get(index).copied().unwrap_or(true) {
            continue;
        }
        out.extend(
            messages
                .get(unit.start..unit.end)
                .unwrap_or_default()
                .iter()
                .cloned(),
        );
    }
    out
}
