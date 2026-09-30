//! Porta de aleatoriedade (`Rng`) — usada **só** para jitter, nunca para decisões.

/// Porta de aleatoriedade.
///
/// O determinismo é requisito: nenhuma decisão pode depender do RNG.
pub trait Rng: Send + Sync {
    /// Próximo valor pseudoaleatório.
    fn next_u64(&mut self) -> u64;
}

/// RNG sequencial determinístico, para testes.
#[derive(Debug, Clone, Copy)]
pub struct SeqRng {
    next: u64,
}

impl SeqRng {
    /// Cria um RNG que devolve uma sequência crescente a partir de `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { next: seed }
    }
}

impl Rng for SeqRng {
    fn next_u64(&mut self) -> u64 {
        let _span = crate::trace_fn!("ports::rng::next_u64");

        let value = self.next;
        self.next = self.next.wrapping_add(1);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{Rng, SeqRng};

    #[test]
    fn seq_rng_is_deterministic() {
        let mut first = SeqRng::new(0);
        let mut second = SeqRng::new(0);
        assert_eq!(first.next_u64(), second.next_u64());
        assert_eq!(first.next_u64(), second.next_u64());
    }
}
