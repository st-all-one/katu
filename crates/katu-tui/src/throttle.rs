//! Orçamento de render (E10-T03): limita a **frequência** de desenho de quadros.
//!
//! O `ratatui` já faz **render diferencial** (escreve só as células que mudam); aqui governa-se
//! **quando** desenhar, para que um fluxo de deltas não provoque um quadro por delta. A decisão usa
//! a porta [`Clock`] (determinismo — nada de `Instant::now`) e pode ser forçada em mudanças
//! estruturais (entrada do utilizador, fim de turno).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use katu_core::ports::Clock;

/// Intervalo mínimo entre quadros (ms): ~60 fps.
pub(crate) const FRAME_INTERVAL_MS: u64 = 16;

/// Limitador de frequência de quadros guiado por [`Clock`].
///
/// É `&self` (usa atómicos), pelo que o loop de eventos e o [`Painter`](crate::Painter) o podem
/// partilhar sem sincronização externa.
pub(crate) struct Throttle<'c> {
    clock: &'c dyn Clock,
    interval_ms: u64,
    last_ms: AtomicU64,
    pending: AtomicBool,
}

impl<'c> Throttle<'c> {
    /// Cria um limitador; o primeiro quadro é sempre devido.
    #[must_use]
    pub(crate) const fn new(clock: &'c dyn Clock, interval_ms: u64) -> Self {
        Self {
            clock,
            interval_ms,
            last_ms: AtomicU64::new(0),
            pending: AtomicBool::new(true),
        }
    }

    /// `true` se já passou o intervalo (ou há um quadro forçado); consome a marca de forçado.
    pub(crate) fn due(&self) -> bool {
        let now = self.clock.now().as_millis();
        let forced = self.pending.swap(false, Ordering::Relaxed);
        if forced || now.saturating_sub(self.last_ms.load(Ordering::Relaxed)) >= self.interval_ms {
            self.last_ms.store(now, Ordering::Relaxed);
            return true;
        }
        false
    }

    /// Força o próximo quadro (mudança estrutural: entrada, fim de turno).
    pub(crate) fn request(&self) {
        self.pending.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use katu_core::ports::{Clock, Timestamp};

    use super::{FRAME_INTERVAL_MS, Throttle};

    /// Relógio de teste que só avança quando mandado (determinístico).
    struct TickingClock {
        now: AtomicU64,
    }

    impl TickingClock {
        fn new(millis: u64) -> Self {
            Self {
                now: AtomicU64::new(millis),
            }
        }

        fn set(&self, millis: u64) {
            self.now.store(millis, Ordering::Relaxed);
        }
    }

    impl Clock for TickingClock {
        fn now(&self) -> Timestamp {
            Timestamp::from_millis(self.now.load(Ordering::Relaxed))
        }
    }

    #[test]
    fn throttles_until_the_interval_elapses() {
        let clock = TickingClock::new(1_000);
        let throttle = Throttle::new(&clock, FRAME_INTERVAL_MS);
        assert!(throttle.due(), "o primeiro quadro é devido");
        clock.set(1_005);
        assert!(!throttle.due(), "dentro do intervalo");
        clock.set(1_016);
        assert!(throttle.due(), "o intervalo passou");
        clock.set(1_020);
        assert!(!throttle.due());
    }

    #[test]
    fn request_forces_the_next_frame() {
        let clock = TickingClock::new(1_000);
        let throttle = Throttle::new(&clock, FRAME_INTERVAL_MS);
        assert!(throttle.due());
        assert!(!throttle.due());
        throttle.request();
        assert!(throttle.due(), "forçado");
        assert!(!throttle.due(), "a marca foi consumida");
    }
}
