//! Argumentos de `memo ask` e `memo knowledge` (E20-T06): paridade com o `kd`.
//!
//! Vivem num módulo filho para manter `memo.rs` sob o teto de linhas.

use clap::Args;

/// Argumentos de `memo ask` (espelha `kd ask`).
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de CLI independentes (modos + filtros)"
)]
#[derive(Debug, Clone, Default, Args)]
pub(crate) struct AskArgs {
    /// Consulta (posicional = body; `-`/ausente lê `stdin`).
    pub(crate) query: Option<String>,
    /// Recupera os corpos dos ids (repetível; aceita vírgula: `--id a,b`).
    #[arg(long = "id", value_name = "ID", value_delimiter = ',')]
    pub(crate) ids: Vec<String>,
    /// Expande o grafo a partir do id.
    #[arg(long, value_name = "ID")]
    pub(crate) around: Option<String>,
    /// Aresta do expand.
    #[arg(long, value_name = "ARESTA")]
    pub(crate) via: Option<String>,
    /// Profundidade do expand.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub(crate) depth: u8,
    /// Saída mínima (`id|statement`).
    #[arg(long)]
    pub(crate) brief: bool,
    /// Inclui notas de trabalho (com `scope`).
    #[arg(long)]
    pub(crate) with_task: bool,
    /// Inclui o corpo completo dos hits.
    #[arg(long)]
    pub(crate) full_content: bool,
    /// Filtro por tipo (repetível; aceita vírgula).
    #[arg(long = "type", value_name = "TIPO", value_delimiter = ',')]
    pub(crate) types: Vec<String>,
    /// Filtro por classificação (repetível; aceita vírgula).
    #[arg(long = "class", value_name = "CLASSE", value_delimiter = ',')]
    pub(crate) classes: Vec<String>,
    /// Filtro por tag (repetível; aceita vírgula).
    #[arg(long = "tag", value_name = "TAG", value_delimiter = ',')]
    pub(crate) tags: Vec<String>,
    /// Filtro por status.
    #[arg(long, value_name = "STATUS")]
    pub(crate) status: Option<String>,
    /// Filtro por escopo (épico).
    #[arg(long = "scope", value_name = "ID")]
    pub(crate) scope: Option<String>,
    /// Filtro por âncora (repetível; aceita vírgula).
    #[arg(long, value_name = "PATH", value_delimiter = ',')]
    pub(crate) anchor: Vec<String>,
    /// Início do intervalo.
    #[arg(long, value_name = "TS")]
    pub(crate) since: Option<String>,
    /// Fim do intervalo.
    #[arg(long, value_name = "TS")]
    pub(crate) until: Option<String>,
    /// Reconstrói o corpus num instante (RFC3339 ou data).
    #[arg(long = "as-of", value_name = "TS")]
    pub(crate) as_of: Option<String>,
    /// Limite de resultados.
    #[arg(long, value_name = "N")]
    pub(crate) limit: Option<usize>,
    /// Modo ranking (notas mais confiáveis, sem query).
    #[arg(long)]
    pub(crate) rank: bool,
    /// Modo vocabulário de tags (`tag|count`).
    #[arg(long = "tags")]
    pub(crate) tags_vocab: bool,
    /// Modo sugestões semânticas de aresta/contradição.
    #[arg(long)]
    pub(crate) suggest: bool,
    /// Varredura explícita do projeto inteiro.
    #[arg(long)]
    pub(crate) universe: bool,
    /// Nº máximo de vizinhos por nota (`--suggest`).
    #[arg(long = "top-k", value_name = "N", default_value_t = 5)]
    pub(crate) top_k: usize,
    /// Relação (`duplicate`/`contradiction`/`link`).
    #[arg(long, value_name = "RELAÇÃO")]
    pub(crate) relation: Option<String>,
    /// Config universal do comando (JSON; XOR com as flags explícitas).
    #[arg(long)]
    pub(crate) params: Option<String>,
    /// Lote JSONL (uma linha = um item; XOR com `--params` e flags).
    #[arg(long)]
    pub(crate) batch: Option<String>,
}

/// Argumentos de `memo knowledge` (espelha `kd map`; sem `--semantic`/`--write` — adiados).
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de CLI independentes (`--members`/`--universe`)"
)]
#[derive(Debug, Clone, Default, Args)]
pub(crate) struct KnowledgeArgs {
    /// Eixo do mapa (`anchor`/`type`/`classification`/`scope`).
    #[arg(long, value_name = "EIXO")]
    pub(crate) axis: Option<String>,
    /// Restringe aos membros de um escopo (épico).
    #[arg(long, value_name = "ESCOPO")]
    pub(crate) scope: Option<String>,
    /// Inclui os membros de cada cluster.
    #[arg(long)]
    pub(crate) members: bool,
    /// Filtro por tipo (repetível; aceita vírgula).
    #[arg(long = "type", value_name = "TIPO", value_delimiter = ',')]
    pub(crate) types: Vec<String>,
    /// Filtro por classificação (repetível; aceita vírgula).
    #[arg(long = "class", value_name = "CLASSE", value_delimiter = ',')]
    pub(crate) classes: Vec<String>,
    /// Filtro por tag (repetível; aceita vírgula).
    #[arg(long = "tag", value_name = "TAG", value_delimiter = ',')]
    pub(crate) tags: Vec<String>,
    /// Filtro por âncora (repetível; aceita vírgula).
    #[arg(long, value_name = "PATH", value_delimiter = ',')]
    pub(crate) anchor: Vec<String>,
    /// Vizinhança de uma nota pelo grafo.
    #[arg(long, value_name = "ID")]
    pub(crate) around: Option<String>,
    /// Profundidade da vizinhança de `--around`.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub(crate) depth: u8,
    /// Varredura explícita do projeto inteiro.
    #[arg(long)]
    pub(crate) universe: bool,
    /// Limite de clusters.
    #[arg(long, value_name = "N")]
    pub(crate) limit: Option<usize>,
}
