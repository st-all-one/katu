//! Codec binário do índice invertido (ADR 0009): postings delta+varint e Bloom por segmento.
//!
//! Formato (`KAI1` + versão implícita): magic, `bloom_len` varint, bits do Bloom, `term_count`
//! varint e, por termo (ordem canónica), `term_len`+bytes, `count` e postings com `field` (u8),
//! `ln` em **delta** e `pos` absoluto (varint). Sem compressão de campo, porque `field` já é 1 byte.

use std::collections::BTreeMap;

use super::bloom::Bloom;
use super::index::{Index, Posting};

/// Marca do formato (`Katu Audit Index`).
const MAGIC: &[u8; 4] = b"KAI1";

/// Serializa índice + Bloom.
pub(super) fn encode(index: &Index, bloom: &Bloom) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_varint(
        &mut out,
        u64::try_from(bloom.bits().len()).unwrap_or(u64::MAX),
    );
    out.extend_from_slice(bloom.bits());
    put_varint(
        &mut out,
        u64::try_from(index.postings().len()).unwrap_or(u64::MAX),
    );
    for (term, postings) in index.postings() {
        put_varint(&mut out, u64::try_from(term.len()).unwrap_or(u64::MAX));
        out.extend_from_slice(term.as_bytes());
        put_varint(&mut out, u64::try_from(postings.len()).unwrap_or(u64::MAX));
        let mut previous = 0_u64;
        for posting in postings {
            out.push(posting.field);
            let ln = u64::from(posting.ln);
            put_varint(&mut out, ln.saturating_sub(previous));
            previous = ln;
            put_varint(&mut out, u64::from(posting.pos));
        }
    }
    out
}

/// Desserializa; `None` se o formato não for reconhecido (o chamador reconstrói).
pub(super) fn decode(bytes: &[u8]) -> Option<(Index, Bloom)> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(4)? != MAGIC.as_slice() {
        return None;
    }
    let bloom_len = usize::try_from(cursor.varint()?).ok()?;
    let bloom = Bloom::from_bits(cursor.take(bloom_len)?.to_vec());
    let terms = cursor.varint()?;
    let mut postings: BTreeMap<String, Vec<Posting>> = BTreeMap::new();
    for _ in 0..terms {
        let len = usize::try_from(cursor.varint()?).ok()?;
        let term = std::str::from_utf8(cursor.take(len)?).ok()?.to_string();
        let count = cursor.varint()?;
        let mut list = Vec::new();
        let mut ln = 0_u64;
        for _ in 0..count {
            let field = cursor.byte()?;
            ln = ln.checked_add(cursor.varint()?)?;
            let pos = cursor.varint()?;
            list.push(Posting {
                field,
                ln: u32::try_from(ln).ok()?,
                pos: u32::try_from(pos).ok()?,
            });
        }
        postings.insert(term, list);
    }
    Some((Index::from_postings(postings), bloom))
}

/// Cursor de leitura com verificações (sem indexação por `[]`).
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.pos)?;
        self.pos = self.pos.checked_add(1)?;
        Some(byte)
    }

    fn varint(&mut self) -> Option<u64> {
        let mut result = 0_u64;
        let mut shift = 0_u32;
        loop {
            let byte = self.byte()?;
            let low = u64::from(byte & 0x7f);
            result |= low.checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift = shift.checked_add(7)?;
            if shift >= 64 {
                return None;
            }
        }
    }
}

/// Escreve um varint LEB128.
fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let low = u8::try_from(value & 0x7f).unwrap_or(0);
        value = value.checked_shr(7).unwrap_or(0);
        if value == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

#[cfg(test)]
mod tests;
