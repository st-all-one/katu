//! Views de `read` (E06-T03): tipos, metadados e despacho para os construtores de relatório.

mod helpers;
mod reports;

use katu_core::report::{ToolReport, content_hash, content_id};

use super::{LineRange, ReadBudget};

/// View de leitura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Conteúdo completo (truncado).
    Full,
    /// Range de linhas.
    Range,
    /// Só símbolos.
    Outline,
    /// Outline + imports + flags (default).
    Summary,
    /// Corpo de um símbolo.
    Symbol,
    /// Diff contra a versão anterior (`base`).
    Diff,
}

impl View {
    /// Interpreta o nome da view.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "full" => Some(Self::Full),
            "range" => Some(Self::Range),
            "outline" => Some(Self::Outline),
            "summary" => Some(Self::Summary),
            "symbol" => Some(Self::Symbol),
            "diff" => Some(Self::Diff),
            _ => None,
        }
    }

    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Range => "range",
            Self::Outline => "outline",
            Self::Summary => "summary",
            Self::Symbol => "symbol",
            Self::Diff => "diff",
        }
    }
}

/// Entrada de uma construção de view.
#[derive(Clone, Copy)]
pub(super) struct Build<'a> {
    /// View pedida.
    pub view: View,
    /// Caminho (relativo).
    pub path: &'a str,
    /// Texto do ficheiro.
    pub text: &'a str,
    /// Bytes originais (para o hash).
    pub bytes: &'a [u8],
    /// Range (para [`View::Range`]).
    pub range: Option<LineRange>,
    /// Símbolo (para [`View::Symbol`]).
    pub symbol: Option<&'a str>,
    /// Versão anterior do conteúdo (para [`View::Diff`]).
    pub base: Option<&'a str>,
    /// Orçamento.
    pub budget: ReadBudget,
}

/// Metadados derivados do ficheiro (id/hash/loc).
pub(super) struct Meta<'a> {
    /// Caminho.
    pub path: &'a str,
    /// Linhas.
    pub loc: u64,
    /// Id content-addressed.
    pub id: String,
    /// Hash do conteúdo.
    pub hash: String,
}

/// Constrói o relatório da view (ou `None` se indisponível).
#[must_use]
pub(super) fn build(input: Build<'_>) -> Option<ToolReport> {
    let lines: Vec<&str> = input.text.lines().collect();
    let meta = Meta {
        path: input.path,
        loc: helpers::len_u64(lines.len()),
        id: content_id("f", input.path.as_bytes()),
        hash: content_hash(input.bytes),
    };
    let report = match input.view {
        View::Full => reports::full(&lines, &meta, input.budget),
        View::Range => {
            let span = input.range.unwrap_or_else(|| LineRange {
                start: 1,
                end: u32::try_from(meta.loc).unwrap_or(u32::MAX),
            });
            reports::range_report(&lines, &meta, span)
        }
        View::Outline => reports::outline_report(&lines, &meta),
        View::Summary => reports::summary(&lines, &meta),
        View::Symbol => reports::symbol(&lines, &meta, input.symbol.unwrap_or("")),
        View::Diff => {
            let base = input.base?;
            reports::diff(base, input.text, &meta)
        }
    };
    Some(report)
}
