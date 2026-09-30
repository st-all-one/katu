//! Entrada de `memo ask` (E20-T06): flags/`--params` → `QueryReq`.
//!
//! Vive num módulo filho para manter `ask.rs` sob o teto de linhas.

use katu_core::error::Error;
use katu_core::memory::{
    Anchor, NoteType, QueryFilter, QueryMode, QueryReq, QueryUniverse, Status,
};
use serde::Deserialize;
use serde_json::Value;

use crate::cli::input;

use super::super::args::AskArgs;

/// Parâmetros de uma consulta (`--params`/linha de `--batch`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct AskParams {
    pub(super) query: Option<String>,
    pub(super) ids: Vec<String>,
    pub(super) around: Option<String>,
    pub(super) via: Option<String>,
    pub(super) depth: Option<u8>,
    pub(super) brief: Option<bool>,
    pub(super) with_task: Option<bool>,
    pub(super) full_content: Option<bool>,
    pub(super) types: Vec<String>,
    pub(super) classes: Vec<String>,
    pub(super) tags: Vec<String>,
    pub(super) status: Option<String>,
    pub(super) scope: Option<String>,
    pub(super) anchor: Vec<String>,
    pub(super) since: Option<String>,
    pub(super) until: Option<String>,
    pub(super) as_of: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) rank: Option<bool>,
    pub(super) tags_vocab: Option<bool>,
    pub(super) suggest: Option<bool>,
    pub(super) universe: Option<bool>,
    pub(super) top_k: Option<usize>,
    pub(super) relation: Option<String>,
}

/// Entrada efetiva (flags ou `--params`), já resolvida.
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de CLI resolvidas (modos + filtros)"
)]
pub(super) struct Input {
    query: Option<String>,
    ids: Vec<String>,
    around: Option<String>,
    via: Option<String>,
    depth: u8,
    brief: bool,
    with_task: bool,
    full_content: bool,
    types: Vec<String>,
    classes: Vec<String>,
    tags: Vec<String>,
    status: Option<String>,
    scope: Option<String>,
    anchor: Vec<String>,
    as_of: Option<String>,
    limit: Option<usize>,
    rank: bool,
    tags_vocab: bool,
    suggest: bool,
    universe: bool,
    top_k: usize,
    relation: Option<String>,
}

impl Input {
    /// Entrada a partir das flags explícitas.
    pub(super) fn from_flags(args: &AskArgs) -> Self {
        Self {
            query: args.query.clone(),
            ids: args.ids.clone(),
            around: args.around.clone(),
            via: args.via.clone(),
            depth: args.depth,
            brief: args.brief,
            with_task: args.with_task,
            full_content: args.full_content,
            types: args.types.clone(),
            classes: args.classes.clone(),
            tags: args.tags.clone(),
            status: args.status.clone(),
            scope: args.scope.clone(),
            anchor: args.anchor.clone(),
            as_of: args.as_of.clone(),
            limit: args.limit,
            rank: args.rank,
            tags_vocab: args.tags_vocab,
            suggest: args.suggest,
            universe: args.universe,
            top_k: args.top_k,
            relation: args.relation.clone(),
        }
    }

    /// Entrada a partir de `--params`/linha de lote.
    pub(super) fn from_params(params: AskParams) -> Self {
        Self {
            query: params.query,
            ids: params.ids,
            around: params.around,
            via: params.via,
            depth: params.depth.unwrap_or(1),
            brief: params.brief.unwrap_or(false),
            with_task: params.with_task.unwrap_or(false),
            full_content: params.full_content.unwrap_or(false),
            types: params.types,
            classes: params.classes,
            tags: params.tags,
            status: params.status,
            scope: params.scope,
            anchor: params.anchor,
            as_of: params.as_of,
            limit: params.limit,
            rank: params.rank.unwrap_or(false),
            tags_vocab: params.tags_vocab.unwrap_or(false),
            suggest: params.suggest.unwrap_or(false),
            universe: params.universe.unwrap_or(false),
            top_k: params.top_k.unwrap_or(5),
            relation: params.relation,
        }
    }

    /// Modo efetivo (a ordem espelha o `kd ask`).
    fn mode(&self) -> QueryMode {
        if self.rank {
            QueryMode::Rank
        } else if self.tags_vocab {
            QueryMode::Tags
        } else if self.suggest {
            QueryMode::Suggest
        } else if !self.ids.is_empty() {
            QueryMode::Get
        } else if self.around.is_some() {
            QueryMode::Expand
        } else {
            QueryMode::Recall
        }
    }

    /// Converte em `QueryReq` (aplica o default de limite).
    pub(super) fn to_query(&self, default_limit: usize) -> Result<QueryReq, Error> {
        let mode = self.mode();
        let query = match mode {
            QueryMode::Recall => input::resolve(self.query.as_deref())?,
            _ => self.query.clone().unwrap_or_default(),
        };
        let filter = QueryFilter {
            types: parse_types(&self.types)?,
            classes: self.classes.clone(),
            statuses: match &self.status {
                Some(raw) => vec![parse_status(raw)?],
                None => Vec::new(),
            },
            tags: self.tags.clone(),
            anchors: self
                .anchor
                .iter()
                .map(|path| Anchor::new(path.clone()))
                .collect(),
            scope: self.scope.clone(),
        };
        Ok(QueryReq {
            mode,
            query,
            ids: self.ids.clone(),
            around: self.around.clone(),
            via: self.via.clone(),
            depth: self.depth,
            filter,
            universe: if self.with_task || self.universe {
                QueryUniverse::All
            } else {
                QueryUniverse::Knowledge
            },
            limit: self.limit.unwrap_or(default_limit),
            brief: self.brief,
            full_content: self.full_content,
            as_of: self.as_of.clone(),
            top_k: self.top_k,
            relation: self.relation.clone(),
            ..QueryReq::default()
        })
    }
}

/// Converte uma lista de tipos fechados.
pub(super) fn parse_types(values: &[String]) -> Result<Vec<NoteType>, Error> {
    values.iter().map(|raw| parse_note_type(raw)).collect()
}

/// Converte um tipo fechado.
fn parse_note_type(raw: &str) -> Result<NoteType, Error> {
    serde_json::from_value(Value::String(raw.to_string()))
        .map_err(|_| Error::invalid_input(format!("tipo inválido: `{raw}`")))
}

/// Converte um estado fechado.
fn parse_status(raw: &str) -> Result<Status, Error> {
    serde_json::from_value(Value::String(raw.to_string()))
        .map_err(|_| Error::invalid_input(format!("status inválido: `{raw}`")))
}

#[cfg(test)]
mod tests {
    use katu_core::memory::{NoteType, QueryMode, QueryUniverse};

    use super::{AskParams, Input};

    fn input(params: AskParams) -> Input {
        Input::from_params(params)
    }

    #[test]
    fn mode_follows_the_kd_order() {
        assert_eq!(input(AskParams::default()).mode(), QueryMode::Recall);
        assert_eq!(
            input(AskParams {
                rank: Some(true),
                ..AskParams::default()
            })
            .mode(),
            QueryMode::Rank
        );
        assert_eq!(
            input(AskParams {
                tags_vocab: Some(true),
                ..AskParams::default()
            })
            .mode(),
            QueryMode::Tags
        );
        assert_eq!(
            input(AskParams {
                suggest: Some(true),
                ..AskParams::default()
            })
            .mode(),
            QueryMode::Suggest
        );
        assert_eq!(
            input(AskParams {
                ids: vec!["n1".to_string()],
                ..AskParams::default()
            })
            .mode(),
            QueryMode::Get
        );
        assert_eq!(
            input(AskParams {
                around: Some("n1".to_string()),
                ..AskParams::default()
            })
            .mode(),
            QueryMode::Expand
        );
    }

    #[test]
    fn filter_parses_closed_types_and_rejects_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let query = input(AskParams {
            types: vec!["decision".to_string()],
            status: Some("active".to_string()),
            ..AskParams::default()
        })
        .to_query(5)?;
        assert_eq!(query.filter.types, vec![NoteType::Decision]);
        assert_eq!(query.limit, 5);

        let refused = input(AskParams {
            types: vec!["nao-existe".to_string()],
            ..AskParams::default()
        })
        .to_query(5);
        assert!(refused.is_err());
        Ok(())
    }

    #[test]
    fn with_task_widens_the_universe() -> Result<(), Box<dyn std::error::Error>> {
        let query = input(AskParams {
            with_task: Some(true),
            ..AskParams::default()
        })
        .to_query(5)?;
        assert_eq!(query.universe, QueryUniverse::All);
        Ok(())
    }
}
