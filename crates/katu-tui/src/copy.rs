//! Cópia por seleção de rato via **OSC 52** (E20-T14).
//!
//! A seleção é acompanhada em coordenadas de ecrã; ao soltar o botão, o texto é lido do buffer
//! renderizado e escrito no clipboard via OSC 52 (`ESC ] 52 ; c ; base64 BEL`), **sem dependência
//! nova**. Um terminal sem suporte simplesmente ignora a sequência; o `katu-tui` mantém o firewall
//! (nada de rede).

use std::io::{self, Write};

use katu_core::diag::{Level, events};
use ratatui::buffer::Buffer;

/// Ponto de ecrã (coluna, linha).
pub(crate) type Point = (u16, u16);

/// Estado da seleção de rato (início e fim).
#[derive(Debug, Default)]
pub(crate) struct Selection {
    start: Option<Point>,
    end: Option<Point>,
}

impl Selection {
    /// Inicia a seleção no ponto.
    pub(crate) fn start(&mut self, point: Point) {
        let _span = katu_core::trace_fn!("copy::start");

        self.start = Some(point);
        self.end = Some(point);
    }

    /// Estende a seleção até ao ponto (só depois de iniciada).
    pub(crate) fn drag(&mut self, point: Point) {
        let _span = katu_core::trace_fn!("copy::drag");

        if self.start.is_some() {
            self.end = Some(point);
        }
    }

    /// Consome a seleção (início, fim), se houver.
    pub(crate) fn take(&mut self) -> Option<(Point, Point)> {
        let _span = katu_core::trace_fn!("copy::take");

        let start = self.start.take()?;
        let end = self.end.take()?;
        Some((start, end))
    }
}

/// Extrai o texto entre dois pontos do buffer renderizado (ordem normalizada).
pub(crate) fn extract(buffer: &Buffer, start: Point, end: Point) -> String {
    let _span = katu_core::trace_fn!("copy::extract");

    let (top, bottom) = if start.1 <= end.1 {
        (start, end)
    } else {
        (end, start)
    };
    let last_col = buffer.area.width.saturating_sub(1);
    let mut lines: Vec<String> = Vec::new();
    for row in top.1..=bottom.1 {
        let (from, to) = bounds(row, top, bottom, last_col);
        let mut line = String::new();
        for col in from..=to {
            if let Some(cell) = buffer.cell((col, row)) {
                line.push_str(cell.symbol());
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

/// Intervalo de colunas da linha `row` dentro da seleção.
fn bounds(row: u16, top: Point, bottom: Point, last_col: u16) -> (u16, u16) {
    let _span = katu_core::trace_fn!("copy::bounds");

    if top.1 == bottom.1 {
        if top.0 <= bottom.0 {
            (top.0, bottom.0)
        } else {
            (bottom.0, top.0)
        }
    } else if row == top.1 {
        (top.0, last_col)
    } else if row == bottom.1 {
        (0, bottom.0)
    } else {
        (0, last_col)
    }
}

/// Escreve `text` no clipboard via OSC 52, no `writer` dado.
pub(crate) fn osc52_to(writer: &mut impl Write, text: &str) -> io::Result<()> {
    let _span = katu_core::fn_span!(Level::Debug, events::MOUSE_COPY, "copy::osc52_to");
    if text.is_empty() {
        return Ok(());
    }
    write!(writer, "\x1b]52;c;{}\x07", base64(text.as_bytes()))?;
    writer.flush()
}

/// Escreve `text` no clipboard via OSC 52 no `stdout` do terminal.
pub(crate) fn osc52(text: &str) -> io::Result<()> {
    let _span = katu_core::trace_fn!("copy::osc52");

    osc52_to(&mut io::stdout(), text)
}

/// Codifica em base64 padrão (`+/`, com `=`), sem dependências.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let indices = [
            (triple >> 18) & 0x3F,
            (triple >> 12) & 0x3F,
            (triple >> 6) & 0x3F,
            triple & 0x3F,
        ];
        let len = chunk.len();
        for (position, index) in indices.iter().enumerate() {
            let pad = (position == 2 && len < 2) || (position == 3 && len < 3);
            if pad {
                out.push('=');
            } else {
                let symbol = ALPHABET
                    .get(usize::try_from(*index).unwrap_or(0))
                    .copied()
                    .unwrap_or(b'=');
                out.push(char::from(symbol));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use super::{Selection, base64, extract, osc52_to};

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn osc52_wraps_the_payload() -> Result<(), Box<dyn std::error::Error>> {
        let mut out = Vec::new();
        osc52_to(&mut out, "hi")?;
        assert_eq!(String::from_utf8_lossy(&out), "\x1b]52;c;aGk=\x07");
        Ok(())
    }

    #[test]
    fn selection_extracts_across_lines() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 3));
        buffer.set_string(0, 0, "olá", Style::default());
        buffer.set_string(0, 1, "mundo", Style::default());
        let text = extract(&buffer, (0, 0), (4, 1));
        assert_eq!(text, "olá\nmundo");
    }

    #[test]
    fn selection_normalizes_reversed_anchors() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 2));
        buffer.set_string(0, 0, "abc", Style::default());
        let text = extract(&buffer, (2, 0), (0, 0));
        assert_eq!(text, "abc");
    }

    #[test]
    fn selection_is_consumed_once() {
        let mut selection = Selection::default();
        selection.start((1, 1));
        selection.drag((2, 2));
        assert!(selection.take().is_some());
        assert!(selection.take().is_none(), "a seleção é consumida");
    }
}
