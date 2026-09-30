//! Filtro de Bloom por segmento (ADR 0009): evita abrir segmentos sem o termo.
//!
//! Determinístico (sem RNG): duplo *hashing* FNV-1a. Sem falsos negativos; um falso positivo só
//! custa uma leitura a mais.

/// Número de hashes por termo.
const K: u8 = 4;
/// Bits mínimos do filtro (evita filtros degenerados em segmentos pequenos).
const MIN_BITS: usize = 512;

/// Filtro de Bloom imutável (construído com o segmento).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bloom {
    bits: Vec<u8>,
    k: u8,
}

impl Bloom {
    /// Constrói a partir de um conjunto de termos (≈10 bits por termo, mínimo [`MIN_BITS`]).
    pub(super) fn from_terms<'t>(terms: impl Iterator<Item = &'t str>) -> Self {
        let terms: Vec<&str> = terms.collect();
        let bits = MIN_BITS
            .max(terms.len().saturating_mul(10))
            .next_power_of_two();
        let mut bloom = Self {
            bits: vec![0; bits.div_ceil(8)],
            k: K,
        };
        for term in terms {
            bloom.insert(term);
        }
        bloom
    }

    /// Reconstrói a partir de bits já lidos.
    pub(super) fn from_bits(bits: Vec<u8>) -> Self {
        Self { bits, k: K }
    }

    /// Bytes do filtro.
    pub(super) fn bits(&self) -> &[u8] {
        &self.bits
    }

    /// `true` se o termo **pode** estar no segmento (`false` = garantidamente ausente).
    pub(super) fn might_contain(&self, term: &str) -> bool {
        let (h1, h2) = hashes(term);
        (0..self.k).all(|index| self.test(h1, h2, index))
    }

    /// Insere um termo.
    fn insert(&mut self, term: &str) {
        let (h1, h2) = hashes(term);
        for index in 0..self.k {
            if let Some((byte, mask)) = self.position(h1, h2, index)
                && let Some(slot) = self.bits.get_mut(byte)
            {
                *slot |= mask;
            }
        }
    }

    /// Testa um termo.
    fn test(&self, h1: u64, h2: u64, index: u8) -> bool {
        self.position(h1, h2, index)
            .is_some_and(|(byte, mask)| self.bits.get(byte).is_some_and(|value| value & mask != 0))
    }

    /// Posição `(byte, máscara)` do hash `index`.
    fn position(&self, h1: u64, h2: u64, index: u8) -> Option<(usize, u8)> {
        let bits = u64::try_from(self.bits.len()).ok()?.checked_mul(8)?;
        let position = h1
            .wrapping_add(h2.wrapping_mul(u64::from(index)))
            .checked_rem(bits)?;
        let byte = usize::try_from(position.checked_div(8)?).ok()?;
        let bit = u8::try_from(position.checked_rem(8)?).ok()?;
        let mask = 1_u8.checked_shl(u32::from(bit)).unwrap_or(0);
        Some((byte, mask))
    }
}

/// Duplo hashing determinístico: `(h1, h2)` a partir de FNV-1a.
fn hashes(term: &str) -> (u64, u64) {
    let first = fnv1a(term.as_bytes());
    let second = fnv1a(&first.to_le_bytes());
    (first, if second == 0 { 1 } else { second })
}

/// FNV-1a de 64 bits.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests;
