//! Ports determinísticos (E01-T02).
//!
//! O núcleo **não** conhece o sistema operacional, o terminal, o relógio nem o RNG globais: todo o
//! acesso ao mundo atravessa uma porta. Os testes usam as *fakes*; as implementações reais vivem no
//! binário (adaptadores finos). O diagnóstico estruturado vive em [`crate::diag`].

pub mod clock;
pub mod env;
pub mod fs;
pub mod process;
pub mod rng;

pub use clock::{Clock, FixedClock, Timestamp};
pub use env::{Env, FakeEnv};
pub use fs::{Fs, FsError, MemFs};
pub use process::{ExecRequest, ExecResult, MemProcess, Process, ProcessError};
pub use rng::{Rng, SeqRng};
