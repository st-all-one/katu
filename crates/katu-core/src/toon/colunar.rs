//! Formato colunar D39 (ADR 0005, emenda v3): o *stream* de secções que chega ao modelo.
//!
//! Bytes: `\x1e` (RS) prefixa uma **tabela** de linhas; `\x1d` (GS) prefixa um **bloco literal**;
//! `\x1f` (US) separa células; `\n` termina a linha. **Sem headers**: o esquema (colunas, domínios,
//! modo) vive no prime ([`super::schema`]). Sem `null` (célula vazia = ausente) e sem listas dentro
//! de células.

use std::fmt::Write;

use crate::diag::{Level, events};
use std::borrow::Cow;

/// Prefixo de uma secção de linhas (Record Separator).
const RS: char = '\u{1e}';
/// Prefixo de um bloco literal (Group Separator).
const GS: char = '\u{1d}';
/// Separador de células (Unit Separator).
const US: char = '\u{1f}';

/// Os delimitadores em byte.
///
/// São ASCII (`< 0x80`), logo o byte UTF-8 é o próprio caractere e um índice encontrado num deles
/// é **sempre** fronteira de `char` — é isso que permite sanitizar sem descodificar UTF-8.
const RS_BYTE: u8 = 0x1e;
const GS_BYTE: u8 = 0x1d;
const US_BYTE: u8 = 0x1f;

/// O que a sanitização substitui por espaço.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sanitize {
    /// Célula: separadores de secção/célula **e** quebras de linha.
    Cell,
    /// Linha de bloco literal: só os separadores de secção (a quebra é a do formato).
    Literal,
}

/// Célula escalar de uma linha.
///
/// O texto é um [`Cow`]: uma célula que vem do payload (a maioria) **empresta** a string do
/// [`super::Value`] em vez de a clonar — era a maior fonte de alocações da projeção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell<'a> {
    /// Texto (sanitizado ao emitir).
    Text(Cow<'a, str>),
    /// Inteiro.
    Int(i64),
    /// Booleano (`0`/`1`).
    Bool(bool),
}

impl<'a> Cell<'a> {
    /// Texto (empresta `&str`, aceita `String`).
    #[must_use]
    pub fn text(value: impl Into<Cow<'a, str>>) -> Self {
        let _span = crate::trace_fn!("toon::colunar::text");

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
    pub fn optional(value: Option<&'a str>) -> Self {
        let _span = crate::trace_fn!("toon::colunar::optional");

        Self::Text(Cow::Borrowed(value.unwrap_or_default()))
    }
}

/// Tabela de linhas (colunas no registo de esquema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowTable<'a> {
    /// Nome da secção.
    pub(crate) name: Cow<'a, str>,
    /// Linhas (cada uma com as células das colunas do esquema).
    pub(crate) rows: Vec<Vec<Cell<'a>>>,
}

impl<'a> RowTable<'a> {
    /// Tabela vazia.
    #[must_use]
    pub fn new(name: impl Into<Cow<'a, str>>) -> Self {
        let _span = crate::trace_fn!("toon::colunar::new");

        Self {
            name: name.into(),
            rows: Vec::new(),
        }
    }

    /// Acrescenta uma linha.
    pub fn push(&mut self, row: Vec<Cell<'a>>) {
        let _span = crate::trace_fn!("toon::colunar::push");

        self.rows.push(row);
    }
}

/// Uma secção do *stream*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section<'a> {
    /// Tabela de linhas (`\x1e`).
    Rows(RowTable<'a>),
    /// Bloco literal (`\x1d`): linhas cruas, sem células.
    Literal {
        /// Nome da secção.
        name: Cow<'a, str>,
        /// Linhas cruas (ordem preservada; emprestadas quando o texto já existe).
        lines: Vec<Cow<'a, str>>,
    },
}

impl<'a> Section<'a> {
    /// Bloco literal a partir do nome e do texto (linhas **emprestadas** de `text`).
    #[must_use]
    pub fn literal(name: impl Into<Cow<'a, str>>, text: &'a str) -> Self {
        let _span = crate::trace_fn!("toon::colunar::literal");

        Self::Literal {
            name: name.into(),
            lines: text.lines().map(Cow::Borrowed).collect(),
        }
    }
}

/// Emite o *stream* D39 (secções vazias omitidas; termina em `\n`).
#[must_use]
pub fn emit(sections: &[Section<'_>]) -> String {
    let _span = crate::fn_span!(Level::Trace, events::TOON_EMIT, "toon::colunar::emit");
    // Uma só alocação: o tamanho é conhecido antes de escrever (evita as realocações do crescimento).
    let mut out = String::with_capacity(byte_len(sections));
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

/// Tamanho exato do *stream* (reserva do buffer; `emit` escreve exatamente estes bytes).
pub(super) fn byte_len(sections: &[Section<'_>]) -> usize {
    let _span = crate::trace_fn!("toon::colunar::byte_len");

    let mut total = 0_usize;
    for section in sections {
        match section {
            Section::Rows(table) if !table.rows.is_empty() => {
                total = total
                    .saturating_add(1)
                    .saturating_add(table.name.len())
                    .saturating_add(1);
                for row in &table.rows {
                    for (index, cell) in row.iter().enumerate() {
                        if index > 0 {
                            total = total.saturating_add(1);
                        }
                        total = total.saturating_add(cell_len(cell));
                    }
                    total = total.saturating_add(1);
                }
            }
            Section::Literal { name, lines } if !lines.is_empty() => {
                total = total
                    .saturating_add(1)
                    .saturating_add(name.len())
                    .saturating_add(1);
                for line in lines {
                    // A sanitização preserva o comprimento em bytes (substituições de 1 byte).
                    total = total.saturating_add(line.len()).saturating_add(1);
                }
            }
            Section::Rows(_) | Section::Literal { .. } => {}
        }
    }
    total
}

/// Bytes que a célula ocupa (o texto sanitizado mantém o comprimento).
fn cell_len(cell: &Cell<'_>) -> usize {
    let _span = crate::trace_fn!("toon::colunar::cell_len");

    match cell {
        Cell::Text(text) => text.len(),
        Cell::Bool(_) => 1,
        Cell::Int(number) => int_len(*number),
    }
}

/// Dígitos de um inteiro (com o sinal).
pub(super) fn int_len(value: i64) -> usize {
    let _span = crate::trace_fn!("toon::colunar::int_len");

    let digits = value.unsigned_abs().checked_ilog10().map_or(1, |power| {
        usize::try_from(power).unwrap_or(0).saturating_add(1)
    });
    if value < 0 {
        digits.saturating_add(1)
    } else {
        digits
    }
}

fn emit_rows(out: &mut String, table: &RowTable<'_>) {
    let _span = crate::fn_span!(Level::Trace, events::TOON_EMIT, "toon::colunar::emit_rows");
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
fn emit_cell(out: &mut String, cell: &Cell<'_>) {
    let _span = crate::trace_fn!("toon::colunar::emit_cell");

    match cell {
        Cell::Text(text) => push_sanitized(out, text, Sanitize::Cell),
        Cell::Int(number) => {
            write!(out, "{number}").unwrap_or_default();
        }
        Cell::Bool(flag) => out.push(if *flag { '1' } else { '0' }),
    }
}

fn emit_literal(out: &mut String, name: &str, lines: &[Cow<'_, str>]) {
    let _span = crate::fn_span!(
        Level::Trace,
        events::TOON_EMIT,
        "toon::colunar::emit_literal"
    );
    out.push(GS);
    out.push_str(name);
    out.push('\n');
    for line in lines {
        push_sanitized(out, line, Sanitize::Literal);
        out.push('\n');
    }
}

/// Máscaras SWAR para o teste "existe byte `< 0x20`" (8 bytes por iteração).
const SWAR_THRESHOLD: u64 = 0x2020_2020_2020_2020;
const SWAR_HIGH: u64 = 0x8080_8080_8080_8080;

/// `true` se algum byte é de controlo (`< 0x20` — superconjunto dos delimitadores).
///
/// Testar 8 bytes por vez com SWAR é ~8× menos iterações: numa célula típica de dezenas de bytes,
/// a varredura dominava a emissão em perfil dev. O byte **mais baixo** abaixo do limiar nunca recebe
/// `borrow` (o de baixo não pede emprestado se for `>= 0x20`), logo é sempre detetado: não há falsos
/// negativos. Um falso positivo (outro byte de controlo) só custa a passagem lenta, que é exata.
pub(super) fn has_control_byte(text: &str) -> bool {
    let _span = crate::trace_fn!("toon::colunar::has_control_byte");

    let bytes = text.as_bytes();
    let (chunks, remainder) = bytes.as_chunks::<8>();
    for chunk in chunks {
        let word = u64::from_ne_bytes(*chunk);
        if word.wrapping_sub(SWAR_THRESHOLD) & !word & SWAR_HIGH != 0 {
            return true;
        }
    }
    remainder.iter().any(|byte| *byte < 0x20)
}

/// `true` se o byte é um delimitador a substituir (todos ASCII, logo nunca dentro de um `char`
/// multi-byte).
const fn is_delimiter(byte: u8, mode: Sanitize) -> bool {
    match byte {
        RS_BYTE | GS_BYTE => true,
        US_BYTE | b'\n' | b'\r' => matches!(mode, Sanitize::Cell),
        _ => false,
    }
}

/// Escreve texto sanitizado (delimitadores/quebras → espaço) **numa só passagem**, sem alocar se não
/// houver nada a substituir; o comprimento em bytes é preservado (todos os substituídos são de 1
/// byte).
fn push_sanitized(out: &mut String, text: &str, mode: Sanitize) {
    let _span = crate::trace_fn!("toon::colunar::push_sanitized");

    // Caso comum (nenhum byte de controlo): uma cópia e nada mais.
    if !has_control_byte(text) {
        out.push_str(text);
        return;
    }
    let mut start = 0_usize;
    for (index, byte) in text.bytes().enumerate() {
        if is_delimiter(byte, mode) {
            out.push_str(text.get(start..index).unwrap_or(""));
            out.push(' ');
            start = index.saturating_add(1);
        }
    }
    out.push_str(text.get(start..).unwrap_or(""));
}
