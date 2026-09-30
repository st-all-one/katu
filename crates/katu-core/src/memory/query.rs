//! Consulta rica à memória (E20-T06): filtros, modos e mapa — tipos do katu.
//!
//! Espelha a superfície de `kd ask`/`kd map` sem importar o `knudge-core`: o adaptador in-process
//! (em `katu/src/memory/`) traduz estes tipos fechados. A porta ganha [`Memory::query`].

use super::types::{Anchor, Basis, NoteRef, NoteType, Score, Status};

/// Modo de consulta (espelha `kd ask`/`kd map`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum QueryMode {
    /// Recall textual (default).
    #[default]
    Recall,
    /// Ranking por confiança, sem query (`--rank`).
    Rank,
    /// Vocabulário de tags, `tag|count` (`--tags`).
    Tags,
    /// Sugestões semânticas de aresta/contradição (`--suggest`).
    Suggest,
    /// Recupera corpos por id (`--id`).
    Get,
    /// Expande o grafo a partir de um id (`--around`).
    Expand,
    /// Mapa estrutural de conhecimento (`memo knowledge`).
    Map,
}

impl QueryMode {
    /// Nome estável (diagnóstico).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::Rank => "rank",
            Self::Tags => "tags",
            Self::Suggest => "suggest",
            Self::Get => "get",
            Self::Expand => "expand",
            Self::Map => "map",
        }
    }
}

/// Universo considerado (espelha `Universe` do knudge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum QueryUniverse {
    /// Só conhecimento (esconde notas de trabalho com `scope`).
    #[default]
    Knowledge,
    /// Tudo, incluindo notas de trabalho (`--with-task`).
    All,
}

/// Filtros estruturais de uma consulta (fechados; strings validadas no adaptador).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryFilter {
    /// Tipos aceitos (`--type`).
    pub types: Vec<NoteType>,
    /// Classificações aceitas (`--class`).
    pub classes: Vec<String>,
    /// Estados aceitos (`--status`); vazio = visíveis por omissão.
    pub statuses: Vec<Status>,
    /// Tags exigidas (`--tag`; basta uma).
    pub tags: Vec<String>,
    /// Âncoras exigidas (`--anchor`; basta uma).
    pub anchors: Vec<Anchor>,
    /// Escopo de tarefa (`--scope`).
    pub scope: Option<String>,
}

/// Pedido de consulta rica (superset de [`super::RecallReq`]).
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de CLI independentes (`--brief`/`--full-content`/`--members`)"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryReq {
    /// Modo de consulta.
    pub mode: QueryMode,
    /// Texto livre (recall/rank).
    pub query: String,
    /// Ids a recuperar (`--id`).
    pub ids: Vec<String>,
    /// Id central da vizinhança (`--around`).
    pub around: Option<String>,
    /// Aresta do expand (`--via`).
    pub via: Option<String>,
    /// Profundidade do expand (`--depth`).
    pub depth: u8,
    /// Filtros estruturais.
    pub filter: QueryFilter,
    /// Universo considerado.
    pub universe: QueryUniverse,
    /// Limite de resultados.
    pub limit: usize,
    /// Saída mínima (`id|statement`).
    pub brief: bool,
    /// Inclui o corpo completo dos hits (`--full-content`).
    pub full_content: bool,
    /// Reconstrói o corpus num instante (`--as-of`).
    pub as_of: Option<String>,
    /// Início do intervalo (`--since`).
    pub since: Option<String>,
    /// Fim do intervalo (`--until`).
    pub until: Option<String>,
    /// Vizinhos por nota no modo `--suggest` (`--top-k`).
    pub top_k: usize,
    /// Relação filtrada no modo `--suggest` (`--relation`).
    pub relation: Option<String>,
    /// Eixo do mapa (`--axis`).
    pub axis: Option<String>,
    /// Inclui os membros de cada cluster (`--members`).
    pub members: bool,
}

impl Default for QueryReq {
    fn default() -> Self {
        Self {
            mode: QueryMode::Recall,
            query: String::new(),
            ids: Vec::new(),
            around: None,
            via: None,
            depth: 1,
            filter: QueryFilter::default(),
            universe: QueryUniverse::default(),
            limit: 5,
            brief: false,
            full_content: false,
            as_of: None,
            since: None,
            until: None,
            top_k: 5,
            relation: None,
            axis: None,
            members: false,
        }
    }
}

impl QueryReq {
    /// Consulta de recall simples (compatível com o caminho do agente).
    #[must_use]
    pub fn recall(query: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            limit,
            ..Self::default()
        }
    }
}

/// Resultado de uma consulta: forma + avisos de degradação graciosa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    /// Forma do resultado (depende do modo).
    pub outcome: QueryOutcome,
    /// Avisos (nunca silenciados).
    pub warnings: Vec<String>,
}

/// Forma do resultado de uma consulta.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryOutcome {
    /// Hits (recall/rank/get/expand).
    Hits(Vec<QueryHit>),
    /// Vocabulário de tags.
    Tags(Vec<TagCount>),
    /// Sugestões semânticas.
    Suggestions(Vec<Suggestion>),
    /// Clusters do mapa estrutural.
    Clusters(Vec<Cluster>),
}

/// Hit de uma consulta (nota + afirmação + score + metadados opcionais).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHit {
    /// Referência à nota.
    pub note: NoteRef,
    /// Afirmação.
    pub statement: String,
    /// Score em pontos base.
    pub score: Score,
    /// Base do score.
    pub basis: Basis,
    /// Âncora de código, quando aplicável.
    pub anchor: Option<Anchor>,
    /// Tipo da nota, quando conhecido.
    pub note_type: Option<NoteType>,
    /// Estado da nota, quando conhecido.
    pub status: Option<Status>,
    /// Tags declaradas.
    pub tags: Vec<String>,
    /// Corpo completo (`--full-content`), quando pedido.
    pub body: Option<String>,
}

/// Contagem de uma tag (`--tags`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    /// Tag.
    pub tag: String,
    /// Nº de notas visíveis que a declaram.
    pub count: usize,
}

/// Sugestão semântica (`--suggest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Relação (`duplicate`/`contradiction`/`link`).
    pub relation: String,
    /// Nota de origem.
    pub from: NoteRef,
    /// Nota de destino.
    pub to: NoteRef,
    /// Score em pontos base.
    pub score: Score,
}

/// Cluster do mapa estrutural (`memo knowledge`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    /// Eixo (`anchor`/`type`/`classification`/`scope`).
    pub axis: String,
    /// Chave do eixo.
    pub key: String,
    /// Membros (ids), quando pedidos.
    pub members: Vec<NoteRef>,
}

#[cfg(test)]
mod tests {
    use super::{QueryMode, QueryReq};

    #[test]
    fn default_matches_the_kd_defaults() {
        let req = QueryReq::default();
        assert_eq!(req.mode, QueryMode::Recall);
        assert_eq!(req.limit, 5);
        assert_eq!(req.top_k, 5);
        assert_eq!(req.depth, 1);
    }

    #[test]
    fn recall_helper_keeps_the_defaults() {
        let req = QueryReq::recall("cache", 9);
        assert_eq!(req.query, "cache");
        assert_eq!(req.limit, 9);
        assert_eq!(req.mode, QueryMode::Recall);
    }
}
