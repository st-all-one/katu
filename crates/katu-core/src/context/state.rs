//! Secção de **estado** do prime (Q-04): o modelo vê o modo, as regras que o travam, o orçamento de
//! passos e o *working set* — sem os descobrir por tentativa e erro.
//!
//! O texto é **compacto, determinístico e sem floats** (a mesma vista ⇒ o mesmo texto, pelo que o
//! prompt é reconstruível do log). Tudo o que entra aqui vem de decisões já tomadas (o modo, o
//! conjunto de regras carregado, os ficheiros que o log registou como alterados), não de uma
//! estimativa nova: o estado **não** inventa factos para o modelo.
//!
//! O invariante `Model-visible ⟺ logged` (E04) obriga a registar a secção no log: vive no evento
//! [`crate::kernel::Event::PromptState`], com o texto exato que entrou no `system`.
//!
//! **Custo:** a secção é uma linha por facto e é limitada por [`MAX_SECTION_BYTES`]; a adoção
//! depende de A/B de turnos (`bench/e18/state/`), pelo que o caminho está **desligado** por omissão
//! (config `behavior.prompt_state`).

use std::collections::BTreeSet;

use crate::diag::{Level, events};

/// Versão do esquema da secção (mudar factos/ordem exige incrementar).
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// Máximo de ficheiros do *working set* mostrados.
pub const MAX_WORKING_SET: usize = 8;

/// Teto de bytes da secção inteira.
pub const MAX_SECTION_BYTES: usize = 512;

/// Vista do estado que entra no prime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateView<'a> {
    /// Modo corrente (`plano` ou `execucao`).
    pub mode: &'a str,
    /// Ids das regras `Enforced` ativas (ordem irrelevante: a secção ordena-os).
    pub rules: &'a [String],
    /// Teto de passos do turno (`None` = sem teto declarado). É o **orçamento**, não o que resta:
    /// um valor que muda a cada passo faria o prompt de sistema mudar dentro do turno e destruiria
    /// o cache de prefixo do provider (§1).
    pub steps_max: Option<u32>,
    /// Compactação ligada?
    pub compaction: bool,
    /// Ficheiros já alterados (relativos à raiz, ordem do log).
    pub working_set: &'a [String],
}

/// Renderiza a secção `estado` (determinística; `""` se não houver nada a dizer).
///
/// Formato: um cabeçalho `estado:` e uma linha por facto, para que o modelo o leia como o resto do
/// prime (texto, não JSON).
#[must_use]
pub fn section(view: &StateView<'_>) -> String {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "context::state::section",
        "rules" => view.rules.len(),
        "working_set" => view.working_set.len(),
    );

    let mut out = String::with_capacity(MAX_SECTION_BYTES / 2);
    out.push_str("estado:\n");
    out.push_str("modo ");
    out.push_str(view.mode);
    out.push('\n');
    out.push_str("regras ");
    out.push_str(&join_sorted(view.rules, MAX_WORKING_SET));
    out.push('\n');
    out.push_str("passos ");
    match view.steps_max {
        Some(max) => out.push_str(&max.to_string()),
        None => out.push('?'),
    }
    out.push('\n');
    out.push_str("compactacao ");
    out.push_str(if view.compaction {
        "ligada"
    } else {
        "desligada"
    });
    out.push('\n');
    out.push_str("tocados ");
    out.push_str(&join_recent(view.working_set, MAX_WORKING_SET));
    out.push('\n');
    truncate(out)
}

/// Junta ids por ordem estável (determinismo), com o último elemento cortado se exceder o teto.
fn join_sorted(items: &[String], max: usize) -> String {
    let _span = crate::trace_fn!("context::state::join_sorted");

    if items.is_empty() {
        return "-".to_string();
    }
    let unique: BTreeSet<&str> = items.iter().map(String::as_str).collect();
    join(unique.into_iter(), max)
}

/// Junta os caminhos **mais recentes** (a cauda da ordem do log é o que o modelo acabou de tocar).
fn join_recent(items: &[String], max: usize) -> String {
    let _span = crate::trace_fn!("context::state::join_recent");

    if items.is_empty() {
        return "-".to_string();
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut recent: Vec<&str> = Vec::new();
    for item in items.iter().rev() {
        if seen.insert(item.as_str()) {
            recent.push(item.as_str());
        }
    }
    join(recent.into_iter(), max)
}

/// Junta com vírgulas, com marcador de corte determinístico (`,+N`).
fn join<'a>(items: impl Iterator<Item = &'a str>, max: usize) -> String {
    let _span = crate::trace_fn!("context::state::join");

    let collected: Vec<&str> = items.collect();
    let shown = collected.len().min(max);
    let mut out = String::new();
    for (index, item) in collected.iter().take(shown).enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(item);
    }
    if collected.len() > shown {
        out.push_str(",+");
        out.push_str(&collected.len().saturating_sub(shown).to_string());
    }
    if out.is_empty() {
        out.push('-');
    }
    out
}

/// Corta a secção no teto, em fronteira de caractere (nunca devolve bytes a mais).
fn truncate(text: String) -> String {
    let _span = crate::trace_fn!("context::state::truncate");

    if text.len() <= MAX_SECTION_BYTES {
        return text;
    }
    let mut end = MAX_SECTION_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    text.get(..end).unwrap_or_default().to_string()
}
