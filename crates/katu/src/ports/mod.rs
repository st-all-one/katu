//! Adaptadores finos das portas (E01-T02).
//!
//! São o **único** sítio do binário que toca o sistema operacional. Os `#[allow]` de
//! `disallowed_methods` são intencionais: a borda é aqui. O log estruturado vive em
//! [`crate::diag`].

use std::time::{SystemTime, UNIX_EPOCH};

use katu_core::ports::{Clock, Env, Rng, Timestamp};

mod fs;
mod process;

#[allow(
    unused_imports,
    reason = "adaptador ligado ao loop em E10 (o módulo `ports` é `dead_code` por agora)"
)]
pub(crate) use process::StdProcess;

#[allow(
    unused_imports,
    reason = "adaptadores ligados ao kernel em E04 (o módulo `ports` é `dead_code` por agora)"
)]
pub(crate) use fs::StdFs;

/// Relógio do sistema.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o relógio do SO"
    )]
    fn now(&self) -> Timestamp {
        let _span = katu_core::trace_fn!("ports::now");

        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => {
                let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                Timestamp::from_millis(millis)
            }
            Err(_) => Timestamp::from_millis(0),
        }
    }
}

/// RNG do sistema (apenas para jitter): *splitmix64* semeado pelo relógio.
pub(crate) struct StdRng {
    state: u64,
}

impl StdRng {
    /// Cria um RNG semeado a partir do relógio do SO.
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: semente do RNG a partir do relógio do SO"
    )]
    pub(crate) fn new() -> Self {
        let _span = katu_core::trace_fn!("ports::new");

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let seed = u64::try_from(nanos).unwrap_or(0);
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl Rng for StdRng {
    fn next_u64(&mut self) -> u64 {
        let _span = katu_core::trace_fn!("ports::next_u64");

        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ mixed.wrapping_shr(30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ mixed.wrapping_shr(27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ mixed.wrapping_shr(31)
    }
}

/// Ambiente real do processo.
pub(crate) struct StdEnv;

impl Env for StdEnv {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o ambiente do processo"
    )]
    fn var(&self, key: &str) -> Option<String> {
        let _span = katu_core::trace_fn!("ports::var");

        std::env::var(key).ok()
    }

    fn args(&self) -> Vec<String> {
        let _span = katu_core::trace_fn!("ports::args");

        std::env::args().skip(1).collect()
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o ambiente do processo"
    )]
    fn vars(&self) -> Vec<(String, String)> {
        let _span = katu_core::trace_fn!("ports::vars");

        std::env::vars().collect()
    }
}
