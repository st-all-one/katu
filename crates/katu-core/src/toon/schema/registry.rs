//! Registo de esquemas (ADR 0005/0006): domínios fechados e tabelas por secção.
//!
//! Extraído de [`super`] para manter o ficheiro do registo sob o limite; a fonte de verdade continua
//! a ser [`REGISTRY`].

use super::{ColumnSpec, Domain, Mode, TableSpec};

/// Coluna de domínio livre.
const fn free(name: &'static str) -> ColumnSpec {
    ColumnSpec { name, domain: &[] }
}

/// Coluna de domínio fechado.
const fn dom(name: &'static str, domain: Domain) -> ColumnSpec {
    ColumnSpec { name, domain }
}

/// Tipos de símbolo (`SymbolKind`).
pub(super) const SYMBOL_KINDS: Domain = &[
    "fn", "struct", "enum", "trait", "impl", "mod", "const", "static", "type", "macro", "use",
];
/// Tipos de linha do `grep`.
pub(super) const LINE_KINDS: Domain = &["code", "test", "comment", "import", "string"];
/// Bases de confiança da memória (`Basis`).
pub(super) const BASIS: Domain = &["measured", "inferred"];
/// Marcadores de `read` (`flags`).
pub(super) const FLAG_KINDS: Domain = &["TODO", "FIXME", "XXX", "HACK"];
/// Estados de feature (`FeatureStatus`).
pub(super) const FEATURE_STATUS: Domain = &["pending", "in_progress", "done"];
/// Tipos de entrada de `ls`.
pub(super) const ENTRY_KINDS: Domain = &["file", "dir"];
/// Tipos de mensagem do digest de compactação.
pub(super) const MESSAGE_KINDS: Domain = &["user", "assistant", "tool_call", "tool_result"];
/// Estados de uma verificação (`CheckStatus`).
pub(super) const CHECK_STATUS: Domain = &["pass", "warn", "block"];
/// Tipos de evento da auditoria (ADR 0009).
pub(super) const AUDIT_KINDS: Domain = &[
    "turn",
    "user",
    "assistant",
    "tool_call",
    "tool_result",
    "phase",
    "waiver",
    "plan",
    "command",
    "workspace",
    "verify",
];

/// Registo de todas as secções emitidas (modo + colunas por ordem).
pub(super) const REGISTRY: &[TableSpec] = &[
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
        name: "a",
        mode: Mode::Rows,
        cols: &[
            free("seq"),
            dom("kind", AUDIT_KINDS),
            free("tool"),
            free("path"),
            free("status"),
            free("rule"),
            free("text"),
        ],
    },
    TableSpec {
        name: "t",
        mode: Mode::Rows,
        cols: &[free("term"), free("field"), free("ln"), free("pos")],
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
