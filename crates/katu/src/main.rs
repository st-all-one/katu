//! `katu` — binário: CLI, composição e adaptador in-process do `knudge`.
//!
//! E01 define o esqueleto. O wiring real chega com E04 (kernel) e E03 (memória).

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::SUCCESS
}
