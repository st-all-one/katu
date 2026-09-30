//! `upgrade` (E20-T02): sincronização de versão. O canal ainda **não** existe.
//!
//! Recusa explicitamente (fail-closed, DF4): não se inventa origem nem se afirma uma versão sem
//! artefacto. A sincronização futura será contra GitHub Releases.

use clap::Args;
use katu_core::diag::{Level, events};
use katu_core::error::Error;

use crate::report::Report;

/// Argumentos de `katu upgrade`.
#[derive(Debug, Clone, Args)]
pub(crate) struct UpgradeArgs {
    /// Emite envelope JSON em `stdout`.
    #[arg(long)]
    pub(crate) json: bool,
}

/// Executa `katu upgrade` (recusa enquanto o canal não existir).
pub(crate) fn execute(_args: &UpgradeArgs) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_UPGRADE, "upgrade::execute");
    Report::failed(
        "upgrade",
        &Error::unavailable("canal de atualização não configurado"),
    )
}
