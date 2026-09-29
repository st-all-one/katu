//! `katu` — binário: CLI, composição e adaptador in-process do `knudge`.
//!
//! E01 define o esqueleto, os adaptadores das portas e o diagnóstico estruturado; o wiring real
//! chega com E04 (kernel).

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "binário: sem API externa; os módulos internos usam pub(crate)"
)]

#[cfg(feature = "profile")]
mod diag;

#[allow(dead_code, reason = "adaptadores ligados ao kernel em E04")]
mod ports;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(feature = "profile")]
    setup_diag();
    ExitCode::SUCCESS
}

/// Instala o diagnóstico estruturado quando `KATU_INSTRUMENT` o pede (DF9/E19).
#[cfg(feature = "profile")]
fn setup_diag() {
    use std::sync::Arc;

    use katu_core::diag::{Sink, install};
    use katu_core::ports::Env;

    let env = ports::StdEnv;
    let requested = env
        .var("KATU_INSTRUMENT")
        .is_some_and(|value| value == "1" || value == "true");
    if requested {
        let sink: Arc<dyn Sink> = Arc::new(diag::StderrSink);
        install(sink);
    }
}
