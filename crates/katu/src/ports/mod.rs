//! Adaptadores finos das portas (E01-T02).
//!
//! São o **único** sítio do binário que toca o sistema operacional. Os `#[allow]` de
//! `disallowed_methods` são intencionais: a borda é aqui. O log estruturado vive em
//! [`crate::diag`].

use std::time::{SystemTime, UNIX_EPOCH};

use katu_core::ports::{Clock, Env, Timestamp};

mod fs;
mod process;

#[cfg_attr(
    not(feature = "memory-in-process"),
    allow(
        unused_imports,
        reason = "sem agente/TUI o adaptador de processo não tem consumidor"
    )
)]
pub(crate) use process::StdProcess;

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
