//! Hash canónico **determinístico** (FNV-1a de 64 bits) — um só sítio para a impressão de bytes.
//!
//! Usado onde é preciso comparar *conteúdo* sem o guardar duas vezes: a assinatura de uma chamada
//! de tool (Q-12) e a integridade do estado no snapshot (Q-15). Não é criptográfico e não substitui
//! validação: é um **detetor de divergência**, não uma defesa contra adversário (ADR 0008).
//!
//! Determinismo: sem RNG, sem relógio; a ordem dos bytes é a ordem canónica de quem chama (mapas
//! `BTreeMap`, `Vec` na ordem de inserção). O mesmo conteúdo dá sempre o mesmo valor.

use serde::Serialize;

use crate::diag::{Level, events};

/// Offset basis de FNV-1a (64 bits).
const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// Primo de FNV-1a (64 bits).
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a de 64 bits sobre os bytes dados.
///
/// `wrapping_mul` é o que a norma pede: o hash é um valor de 64 bits, não uma multiplicação exata.
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let _span = crate::fn_span!(
        Level::Trace,
        events::KERNEL_TRANSITION,
        "kernel::hash::fnv1a"
    );
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Hash canónico de um valor serializável (JSON determinístico: chaves ordenadas).
///
/// # Errors
/// Erro de serialização (nunca acontece para os tipos do kernel, que são `Serialize`).
pub fn canonical<T: Serialize>(value: &T) -> Result<u64, serde_json::Error> {
    let _span = crate::trace_fn!("kernel::hash::canonical");

    Ok(fnv1a(&serde_json::to_vec(value)?))
}

#[cfg(test)]
mod tests {
    use super::{canonical, fnv1a};

    #[test]
    fn fnv1a_matches_the_reference_vectors() {
        // Vetores publicados de FNV-1a 64 bits.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn canonical_is_order_sensitive_and_stable() -> Result<(), Box<dyn std::error::Error>> {
        let first = canonical(&vec!["a", "b"])?;
        let second = canonical(&vec!["b", "a"])?;
        assert_ne!(first, second, "a ordem faz parte do conteúdo");
        assert_eq!(canonical(&vec!["a", "b"])?, first, "determinístico");
        Ok(())
    }

    #[test]
    fn canonical_ignores_map_insertion_order() -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::BTreeMap;
        let mut left = BTreeMap::new();
        left.insert("b", 2);
        left.insert("a", 1);
        let mut right = BTreeMap::new();
        right.insert("a", 1);
        right.insert("b", 2);
        assert_eq!(canonical(&left)?, canonical(&right)?);
        Ok(())
    }
}
