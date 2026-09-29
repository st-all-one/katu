//! Porta de relógio (`Clock`) e o tipo [`Timestamp`] (UTC em milissegundos).

/// Instante UTC em milissegundos desde a *epoch* Unix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Constrói um instante a partir de milissegundos.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Milissegundos desde a *epoch*.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

/// Porta de relógio.
///
/// O núcleo **nunca** chama `SystemTime::now` diretamente (reforçado por `clippy.toml`).
pub trait Clock: Send + Sync {
    /// Instante atual.
    fn now(&self) -> Timestamp;
}

/// Relógio fixo, para testes determinísticos.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: Timestamp,
}

impl FixedClock {
    /// Cria um relógio fixo no instante dado.
    #[must_use]
    pub const fn new(now: Timestamp) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock, Timestamp};

    #[test]
    fn fixed_clock_is_constant() {
        let clock = FixedClock::new(Timestamp::from_millis(42));
        assert_eq!(clock.now(), clock.now());
        assert_eq!(clock.now().as_millis(), 42);
    }
}
