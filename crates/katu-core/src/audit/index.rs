//! Índice invertido (ADR 0009) com posições: termos, frases e filtros de campo.
//!
//! A construção é determinística (`BTreeMap`, ordem canónica). A consulta avalia em `O(candidatos)`
//! via postings; as frases usam as **posições** dentro do mesmo campo.

use std::collections::{BTreeMap, BTreeSet};

use super::record::AuditRecord;
use crate::diag::{Level, events};
use crate::plan::matches_glob;

/// Uma ocorrência de termo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Posting {
    /// Campo (0..=5).
    pub field: u8,
    /// Linha do segmento.
    pub ln: u32,
    /// Posição dentro do campo.
    pub pos: u32,
}

/// Índice invertido termo → ocorrências (ordenadas).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    postings: BTreeMap<String, Vec<Posting>>,
}

impl Index {
    /// Constrói o índice a partir das linhas.
    #[must_use]
    pub fn build(records: &[AuditRecord]) -> Self {
        let _span = crate::fn_span!(Level::Trace, events::AUDIT_INDEX, "audit::index::build");
        let mut postings: BTreeMap<String, Vec<Posting>> = BTreeMap::new();
        for (ln, record) in records.iter().enumerate() {
            let ln = u32::try_from(ln).unwrap_or(u32::MAX);
            for (field, text) in record.fields().iter().enumerate() {
                let field = u8::try_from(field).unwrap_or(u8::MAX);
                for (pos, term) in tokenize(text).into_iter().enumerate() {
                    let pos = u32::try_from(pos).unwrap_or(u32::MAX);
                    postings
                        .entry(term)
                        .or_default()
                        .push(Posting { field, ln, pos });
                }
            }
        }
        Self { postings }
    }

    /// Linhas que satisfazem um grupo (todos os termos/frases; filtros exactos).
    #[must_use]
    pub fn matches(&self, records: &[AuditRecord], group: &Group) -> BTreeSet<u32> {
        let _span = crate::fn_span!(Level::Trace, events::AUDIT_QUERY, "audit::index::matches");
        let mut candidates: Option<BTreeSet<u32>> = None;
        for term in &group.terms {
            let docs = self.docs(term);
            candidates = Some(intersect(candidates, docs));
        }
        for phrase in &group.phrases {
            let docs = self.phrase_docs(phrase);
            candidates = Some(intersect(candidates, docs));
        }
        let mut result = candidates.unwrap_or_else(|| all_docs(records));
        result.retain(|ln| {
            usize::try_from(*ln)
                .ok()
                .and_then(|index| records.get(index))
                .is_some_and(|record| record_matches(record, group))
        });
        result
    }

    /// Ocorrências de um termo, indexadas por `(field, ln, pos)`.
    #[must_use]
    pub fn postings(&self) -> &BTreeMap<String, Vec<Posting>> {
        let _span = crate::trace_fn!("audit::index::postings");

        &self.postings
    }

    /// Constrói a partir de postings já ordenados (persistência).
    #[must_use]
    pub fn from_postings(postings: BTreeMap<String, Vec<Posting>>) -> Self {
        let _span = crate::trace_fn!("audit::index::from_postings");

        Self { postings }
    }

    fn docs(&self, term: &str) -> BTreeSet<u32> {
        let _span = crate::trace_fn!("audit::index::docs");

        self.postings
            .get(term)
            .map(|list| list.iter().map(|posting| posting.ln).collect())
            .unwrap_or_default()
    }

    fn phrase_docs(&self, words: &[String]) -> BTreeSet<u32> {
        let _span = crate::trace_fn!("audit::index::phrase_docs");

        let Some(first) = words.first() else {
            return BTreeSet::new();
        };
        let mut out = BTreeSet::new();
        'outer: for start in self.postings.get(first).into_iter().flatten() {
            for (offset, word) in words.iter().enumerate().skip(1) {
                let delta = u32::try_from(offset).unwrap_or(u32::MAX);
                let pos = start.pos.saturating_add(delta);
                let found = self.postings.get(word).is_some_and(|list| {
                    list.iter()
                        .any(|p| p.field == start.field && p.ln == start.ln && p.pos == pos)
                });
                if !found {
                    continue 'outer;
                }
            }
            out.insert(start.ln);
        }
        out
    }
}

/// Grupo de cláusulas em conjunção (um `OR` separa grupos).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    /// Termos obrigatórios.
    pub terms: Vec<String>,
    /// Frases obrigatórias (sequências de termos).
    pub phrases: Vec<Vec<String>>,
    /// Filtros exactos de `kind`.
    pub kinds: Vec<String>,
    /// Filtros exactos de `tool`.
    pub tools: Vec<String>,
    /// Filtros exactos de `status`.
    pub statuses: Vec<String>,
    /// Filtros de caminho (glob).
    pub paths: Vec<String>,
}

/// Consulta: disjunção de grupos (`OR`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Grupos (vazio = sem termos → tudo).
    pub groups: Vec<Group>,
}

impl Query {
    /// Analisa a consulta: `termo`, `"frase"`, `kind:x`, `tool:x`, `status:x`, `path:<glob>`, `OR`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let _span = crate::trace_fn!("audit::index::parse");

        let mut groups: Vec<Group> = vec![Group::default()];
        for clause in clauses(text) {
            if matches!(clause, Clause::Or) {
                groups.push(Group::default());
                continue;
            }
            let Some(group) = groups.last_mut() else {
                continue;
            };
            match clause {
                Clause::Or => {}
                Clause::Phrase(phrase) => group.phrases.push(tokenize(&phrase)),
                Clause::Term(term) => {
                    if let Some((key, value)) = term.split_once(':') {
                        push_filter(group, &key.to_lowercase(), &value.to_lowercase());
                    } else {
                        group.terms.push(term.to_lowercase());
                    }
                }
            }
        }
        Self { groups }
    }
}

fn push_filter(group: &mut Group, key: &str, value: &str) {
    let _span = crate::trace_fn!("audit::index::push_filter");

    match key {
        "kind" => group.kinds.push(value.to_string()),
        "tool" => group.tools.push(value.to_string()),
        "status" => group.statuses.push(value.to_string()),
        "path" => group.paths.push(value.to_string()),
        _ => group.terms.push(format!("{key}:{value}")),
    }
}

/// Cláusula analisada.
enum Clause {
    Term(String),
    Phrase(String),
    Or,
}

fn clauses(text: &str) -> Vec<Clause> {
    let _span = crate::trace_fn!("audit::index::clauses");

    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '"' => {
                if quoted {
                    out.push(Clause::Phrase(std::mem::take(&mut current)));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                flush(&mut out, &mut current);
            }
            c => current.push(c),
        }
    }
    if quoted {
        current.push('"');
    }
    flush(&mut out, &mut current);
    out
}

fn flush(out: &mut Vec<Clause>, current: &mut String) {
    let _span = crate::trace_fn!("audit::index::flush");

    if current.is_empty() {
        return;
    }
    let word = std::mem::take(current);
    if word == "OR" {
        out.push(Clause::Or);
    } else {
        out.push(Clause::Term(word));
    }
}

/// Quebra um texto em termos (minúsculas, `[a-z0-9_]`/alfanumérico).
#[must_use]
pub(super) fn tokenize(text: &str) -> Vec<String> {
    let _span = crate::trace_fn!("audit::index::tokenize");

    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn intersect(current: Option<BTreeSet<u32>>, next: BTreeSet<u32>) -> BTreeSet<u32> {
    let _span = crate::trace_fn!("audit::index::intersect");

    match current {
        Some(set) => set.intersection(&next).copied().collect(),
        None => next,
    }
}

fn all_docs(records: &[AuditRecord]) -> BTreeSet<u32> {
    let _span = crate::trace_fn!("audit::index::all_docs");

    (0..records.len())
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect()
}

fn record_matches(record: &AuditRecord, group: &Group) -> bool {
    let _span = crate::trace_fn!("audit::index::record_matches");

    group.kinds.iter().all(|kind| kind == record.kind)
        && group
            .tools
            .iter()
            .all(|tool| tool == &record.tool.to_lowercase())
        && group
            .statuses
            .iter()
            .all(|status| status == &record.status.to_lowercase())
        && group
            .paths
            .iter()
            .all(|pattern| matches_glob(pattern, &record.path))
}

#[cfg(test)]
mod tests;
