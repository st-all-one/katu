//! Registo de esquema do formato ao modelo (ADR 0005, emenda v3).
//!
//! Com os headers removidos do *stream*, o **prime** é o registo: declara, por secção, o modo
//! (tabela/linhas ou bloco literal) e as colunas por ordem. Fonte única de verdade: a projeção e o
//! prime leem daqui, pelo que não há drift entre o que é emitido e o que é ensinado.

/// Domínio fechado de uma coluna (`&[]` = livre).
pub type Domain = &'static [&'static str];

/// Coluna: nome estável e domínio fechado opcional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    /// Nome da coluna (curto mas claro).
    pub name: &'static str,
    /// Domínio fechado (enums) ou vazio.
    pub domain: Domain,
}

/// Modo de uma secção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Tabela de linhas (`\x1e`), células separadas por `\x1f`.
    Rows,
    /// Bloco literal (`\x1d`): linhas cruas, sem células.
    Literal,
}

/// Esquema de uma secção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    /// Nome da secção.
    pub name: &'static str,
    /// Modo.
    pub mode: Mode,
    /// Colunas (vazio em modo literal).
    pub cols: &'static [ColumnSpec],
}

/// Coluna de domínio livre.
const fn free(name: &'static str) -> ColumnSpec {
    ColumnSpec { name, domain: &[] }
}

/// Coluna de domínio fechado.
const fn dom(name: &'static str, domain: Domain) -> ColumnSpec {
    ColumnSpec { name, domain }
}

/// Tipos de símbolo (`SymbolKind`).
pub const SYMBOL_KINDS: Domain = &[
    "fn", "struct", "enum", "trait", "impl", "mod", "const", "static", "type", "macro", "use",
];
/// Tipos de linha do `grep`.
pub const LINE_KINDS: Domain = &["code", "test", "comment", "import", "string"];
/// Bases de confiança da memória (`Basis`).
pub const BASIS: Domain = &["measured", "inferred"];
/// Marcadores de `read` (`flags`).
pub const FLAG_KINDS: Domain = &["TODO", "FIXME", "XXX", "HACK"];
/// Estados de feature (`FeatureStatus`).
pub const FEATURE_STATUS: Domain = &["pending", "in_progress", "done"];
/// Tipos de entrada de `ls`.
pub const ENTRY_KINDS: Domain = &["file", "dir"];
/// Tipos de mensagem do digest de compactação.
pub const MESSAGE_KINDS: Domain = &["user", "assistant", "tool_call", "tool_result"];
/// Estados de uma verificação (`CheckStatus`).
pub const CHECK_STATUS: Domain = &["pass", "warn", "block"];

/// Registo de todas as secções emitidas (modo + colunas por ordem).
pub fn registry() -> &'static [TableSpec] {
    REGISTRY
}

/// Esquema de uma secção pelo nome (inclui filhos qualificados, ex.: `clusters.hits`).
#[must_use]
pub fn spec(name: &str) -> Option<&'static TableSpec> {
    REGISTRY.iter().find(|spec| spec.name == name)
}

const REGISTRY: &[TableSpec] = &[
    TableSpec {
        name: "r",
        mode: Mode::Rows,
        cols: &[
            free("kind"),
            free("id"),
            free("hash"),
            free("cur"),
            free("tot"),
            free("trunc"),
            free("bytes"),
            free("ms"),
            free("tok"),
        ],
    },
    TableSpec {
        name: "k",
        mode: Mode::Rows,
        cols: &[free("k"), free("v")],
    },
    TableSpec {
        name: "sym",
        mode: Mode::Rows,
        cols: &[free("alias"), free("value")],
    },
    TableSpec {
        name: "m",
        mode: Mode::Rows,
        cols: &[dom("kind", MESSAGE_KINDS), free("text")],
    },
    TableSpec {
        name: "tool",
        mode: Mode::Rows,
        cols: &[free("name"), free("sig")],
    },
    TableSpec {
        name: "checks",
        mode: Mode::Rows,
        cols: &[free("id"), dom("status", CHECK_STATUS), free("detail")],
    },
    TableSpec {
        name: "symbols",
        mode: Mode::Rows,
        cols: &[
            free("id"),
            dom("kind", SYMBOL_KINDS),
            free("name"),
            free("start"),
            free("end"),
        ],
    },
    TableSpec {
        name: "flags",
        mode: Mode::Rows,
        cols: &[free("ln"), dom("kind", FLAG_KINDS)],
    },
    TableSpec {
        name: "imports",
        mode: Mode::Rows,
        cols: &[free("ref")],
    },
    TableSpec {
        name: "features",
        mode: Mode::Rows,
        cols: &[free("id"), dom("status", FEATURE_STATUS), free("desc")],
    },
    TableSpec {
        name: "forbidden_files",
        mode: Mode::Rows,
        cols: &[free("ref")],
    },
    TableSpec {
        name: "allowed_files",
        mode: Mode::Rows,
        cols: &[free("ref")],
    },
    TableSpec {
        name: "files",
        mode: Mode::Rows,
        cols: &[free("path"), free("score")],
    },
    TableSpec {
        name: "entries",
        mode: Mode::Rows,
        cols: &[
            free("path"),
            dom("ty", ENTRY_KINDS),
            free("lang"),
            free("loc"),
            free("syms"),
            free("exports"),
            free("tests"),
        ],
    },
    TableSpec {
        name: "clusters",
        mode: Mode::Rows,
        cols: &[free("sym"), free("name")],
    },
    TableSpec {
        name: "clusters.hits",
        mode: Mode::Rows,
        cols: &[
            free("g"),
            free("path"),
            free("ln"),
            dom("ty", LINE_KINDS),
            free("sym"),
            free("preview"),
        ],
    },
    TableSpec {
        name: "hits",
        mode: Mode::Rows,
        cols: &[
            free("rank"),
            free("note"),
            free("statement"),
            free("score"),
            dom("basis", BASIS),
            free("ev"),
        ],
    },
    TableSpec {
        name: "refs",
        mode: Mode::Rows,
        cols: &[free("ref")],
    },
    TableSpec {
        name: "hunks",
        mode: Mode::Rows,
        cols: &[
            free("old_start"),
            free("old_len"),
            free("new_start"),
            free("new_len"),
        ],
    },
    TableSpec {
        name: "hunks.lines",
        mode: Mode::Literal,
        cols: &[],
    },
    TableSpec {
        name: "argv",
        mode: Mode::Rows,
        cols: &[free("ref")],
    },
    TableSpec {
        name: "next",
        mode: Mode::Rows,
        cols: &[free("v")],
    },
    TableSpec {
        name: "text",
        mode: Mode::Literal,
        cols: &[],
    },
    TableSpec {
        name: "stdout",
        mode: Mode::Literal,
        cols: &[],
    },
    TableSpec {
        name: "stderr",
        mode: Mode::Literal,
        cols: &[],
    },
];

/// Valida o registo: nomes únicos/não vazios, colunas não vazias e literais sem colunas.
///
/// É o gate que impede uma secção nova de sair sem esquema (o prime deixaria de a ensinar).
#[must_use]
pub fn validate() -> Vec<&'static str> {
    let mut issues = Vec::new();
    for (index, spec) in REGISTRY.iter().enumerate() {
        if spec.name.is_empty() {
            issues.push("secção sem nome");
        }
        if REGISTRY
            .iter()
            .take(index)
            .any(|other| other.name == spec.name)
        {
            issues.push("nome de secção duplicado");
        }
        if spec.mode == Mode::Literal && !spec.cols.is_empty() {
            issues.push("bloco literal com colunas");
        }
        if spec.mode == Mode::Rows && spec.cols.is_empty() {
            issues.push("tabela sem colunas");
        }
        for col in spec.cols {
            if col.name.is_empty() {
                issues.push("coluna sem nome");
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests;
