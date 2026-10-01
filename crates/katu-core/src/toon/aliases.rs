//! Aliases de sessão (ADR 0005, emenda v3): ids/paths longos viram `#N`/`@N`.
//!
//! O modelo cita um **handle curto** (menos tokens e menos erros de transcrição); o mapa vive no
//! prime (`sym`) e é reconstruído deterministicamente a partir do log. A substituição é feita ao
//! nível do [`Value`], antes da projeção — a projeção continua pura.
//!
//! **Limiar** ([`ALIAS_THRESHOLD`]): só entidades **quentes** (usadas ≥3×) ganham alias. Declarar um
//! alias custa a string inteira no `sym`; com 1–2 usos é overhead (medido). Assim os ids/paths que
//! aparecem uma só vez ficam crus.

use std::collections::BTreeMap;

use super::Value;
use super::colunar::{Cell, RowTable, Section};

/// Usos mínimos antes de atribuir um alias.
const ALIAS_THRESHOLD: u32 = 3;

/// Chaves cujo valor é um id content-addressed.
const ID_KEYS: &[&str] = &["id", "from_id", "to_id", "note", "sym", "symbol", "ref"];
/// Chaves cujo valor é um caminho.
const PATH_KEYS: &[&str] = &[
    "path", "stored", "original", "root", "from", "to", "cwd", "ev",
];

/// Par `(alias, valor)` recém-registado.
type Fresh = Option<(String, String)>;
/// Lista de pares `(alias, valor)`.
type Pairs = Vec<(String, String)>;

/// Registo de aliases de uma sessão (ids `#N`, paths `@N`).
#[derive(Debug, Default)]
pub struct Aliases {
    ids: BTreeMap<String, String>,
    paths: BTreeMap<String, String>,
    id_uses: BTreeMap<String, u32>,
    path_uses: BTreeMap<String, u32>,
    next_id: u32,
    next_path: u32,
}

impl Aliases {
    /// Registo vazio.
    #[must_use]
    pub fn new() -> Self {
        let _span = crate::trace_fn!("toon::aliases::new");

        Self::default()
    }

    /// `true` se não há aliases.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let _span = crate::trace_fn!("toon::aliases::is_empty");

        self.ids.is_empty() && self.paths.is_empty()
    }

    /// Interna um id: devolve o alias a partir do limiar e, se for novo, o par.
    pub fn intern_id(&mut self, raw: &str) -> (String, Fresh) {
        let _span = crate::trace_fn!("toon::aliases::intern_id");

        if let Some(alias) = self.ids.get(raw) {
            return (alias.clone(), None);
        }
        let uses = self.id_uses.entry(raw.to_string()).or_insert(0);
        *uses = uses.saturating_add(1);
        if *uses < ALIAS_THRESHOLD {
            return (raw.to_string(), None);
        }
        self.next_id = self.next_id.saturating_add(1);
        let alias = format!("#{}", self.next_id);
        drop(self.ids.insert(raw.to_string(), alias.clone()));
        (alias.clone(), Some((alias, raw.to_string())))
    }

    /// Interna um caminho: devolve o alias a partir do limiar e, se for novo, o par.
    pub fn intern_path(&mut self, raw: &str) -> (String, Fresh) {
        let _span = crate::trace_fn!("toon::aliases::intern_path");

        if let Some(alias) = self.paths.get(raw) {
            return (alias.clone(), None);
        }
        let uses = self.path_uses.entry(raw.to_string()).or_insert(0);
        *uses = uses.saturating_add(1);
        if *uses < ALIAS_THRESHOLD {
            return (raw.to_string(), None);
        }
        self.next_path = self.next_path.saturating_add(1);
        let alias = format!("@{}", self.next_path);
        drop(self.paths.insert(raw.to_string(), alias.clone()));
        (alias.clone(), Some((alias, raw.to_string())))
    }

    /// Substitui ids/paths por aliases; devolve o valor e os pares novos.
    #[must_use]
    pub fn substitute(&mut self, value: &Value) -> (Value, Pairs) {
        let _span = crate::trace_fn!("toon::aliases::substitute");

        let mut new = Vec::new();
        let out = self.walk(value, None, &mut new);
        (out, new)
    }

    fn walk(&mut self, value: &Value, key: Option<&str>, new: &mut Pairs) -> Value {
        let _span = crate::trace_fn!("toon::aliases::walk");

        match value {
            Value::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(name, item)| (name.clone(), self.walk(item, Some(name), new)))
                    .collect(),
            ),
            Value::Flow(entries) => Value::Flow(
                entries
                    .iter()
                    .map(|(name, item)| (name.clone(), self.walk(item, Some(name), new)))
                    .collect(),
            ),
            Value::List(items) => {
                Value::List(items.iter().map(|item| self.walk(item, key, new)).collect())
            }
            Value::Str(text) => {
                let Some(key) = key else {
                    return value.clone();
                };
                if ID_KEYS.contains(&key) {
                    let (alias, fresh) = self.intern_id(text);
                    if let Some(pair) = fresh {
                        new.push(pair);
                    }
                    Value::Str(alias)
                } else if PATH_KEYS.contains(&key) {
                    let (alias, fresh) = self.intern_path(text);
                    if let Some(pair) = fresh {
                        new.push(pair);
                    }
                    Value::Str(alias)
                } else {
                    value.clone()
                }
            }
            other => other.clone(),
        }
    }

    /// Secção `sym` com todos os aliases da sessão (para o prime).
    #[must_use]
    pub fn symbol_section(&self) -> Section<'_> {
        let _span = crate::trace_fn!("toon::aliases::symbol_section");

        let mut table = RowTable::new("sym");
        for (alias, value) in self.ids.iter().chain(self.paths.iter()) {
            table.push(vec![Cell::text(alias.as_str()), Cell::text(value.as_str())]);
        }
        Section::Rows(table)
    }
}
