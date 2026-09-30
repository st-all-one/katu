//! Formato colunar D39 (ADR 0005, emenda v3): o *stream* de secções que chega ao modelo.
//!
//! Bytes: `\x1e` (RS) prefixa uma **tabela** de linhas; `\x1d` (GS) prefixa um **bloco literal**;
//! `\x1f` (US) separa células; `\n` termina a linha. **Sem headers**: o esquema (colunas, domínios,
//! modo) vive no prime ([`super::schema`]). Sem `null` (célula vazia = ausente) e sem listas dentro
//! de células.

use std::fmt::Write;

/// Prefixo de uma secção de linhas (Record Separator).
const RS: char = '\u{1e}';
/// Prefixo de um bloco literal (Group Separator).
const GS: char = '\u{1d}';
/// Separador de células (Unit Separator).
const US: char = '\u{1f}';

/// Célula escalar de uma linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// Texto (sanitizado ao emitir).
    Text(String),
    /// Inteiro.
    Int(i64),
    /// Booleano (`0`/`1`).
    Bool(bool),
}

impl Cell {
    /// Texto.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Inteiro.
    #[must_use]
    pub const fn int(value: i64) -> Self {
        Self::Int(value)
    }

    /// Booleano.
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "construtor de escalar booleano: a API é `Cell::bool(bool)`"
    )]
    #[must_use]
    pub const fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    /// Texto opcional (`None` vira célula vazia).
    #[must_use]
    pub fn optional(value: Option<String>) -> Self {
        Self::Text(value.unwrap_or_default())
    }
}

/// Tabela de linhas (colunas no registo de esquema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowTable {
    /// Nome da secção.
    pub(crate) name: String,
    /// Linhas (cada uma com as células das colunas do esquema).
    pub(crate) rows: Vec<Vec<Cell>>,
}

impl RowTable {
    /// Tabela vazia.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rows: Vec::new(),
        }
    }

    /// Acrescenta uma linha.
    pub fn push(&mut self, row: Vec<Cell>) {
        self.rows.push(row);
    }
}

/// Uma secção do *stream*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    /// Tabela de linhas (`\x1e`).
    Rows(RowTable),
    /// Bloco literal (`\x1d`): linhas cruas, sem células.
    Literal {
        /// Nome da secção.
        name: String,
        /// Linhas cruas (ordem preservada).
        lines: Vec<String>,
    },
}

impl Section {
    /// Bloco literal a partir do nome e do texto.
    #[must_use]
    pub fn literal(name: impl Into<String>, text: &str) -> Self {
        Self::Literal {
            name: name.into(),
            lines: text.lines().map(str::to_string).collect(),
        }
    }
}

/// Emite o *stream* D39 (secções vazias omitidas; termina em `\n`).
#[must_use]
pub fn emit(sections: &[Section]) -> String {
    let mut out = String::new();
    for section in sections {
        match section {
            Section::Rows(table) if !table.rows.is_empty() => {
                emit_rows(&mut out, table);
            }
            Section::Literal { name, lines } if !lines.is_empty() => {
                emit_literal(&mut out, name, lines);
            }
            Section::Rows(_) | Section::Literal { .. } => {}
        }
    }
    out
}

fn emit_rows(out: &mut String, table: &RowTable) {
    out.reserve(table.name.len().saturating_add(1));
    out.push(RS);
    out.push_str(&table.name);
    out.push('\n');
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push(US);
            }
            emit_cell(out, cell);
        }
        out.push('\n');
    }
}

/// Escreve uma célula diretamente no buffer (sem alocação intermédia).
fn emit_cell(out: &mut String, cell: &Cell) {
    match cell {
        Cell::Text(text) => push_sanitized(out, text),
        Cell::Int(number) => {
            write!(out, "{number}").unwrap_or_default();
        }
        Cell::Bool(flag) => out.push(if *flag { '1' } else { '0' }),
    }
}

fn emit_literal(out: &mut String, name: &str, lines: &[String]) {
    out.push(GS);
    out.push_str(name);
    out.push('\n');
    for line in lines {
        for ch in line.chars() {
            out.push(match ch {
                RS | GS => ' ',
                other => other,
            });
        }
        out.push('\n');
    }
}

/// Escreve texto sanitizado (delimitadores/quebras → espaço) sem alocar se não houver nada a
/// substituir; o comprimento em bytes é preservado (todos os substituídos são de 1 byte).
fn push_sanitized(out: &mut String, text: &str) {
    if !has_delimiter(text) {
        out.push_str(text);
        return;
    }
    for ch in text.chars() {
        out.push(match ch {
            RS | GS | US | '\n' | '\r' => ' ',
            other => other,
        });
    }
}

fn has_delimiter(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(ch, RS | GS | US | '\n' | '\r'))
}
