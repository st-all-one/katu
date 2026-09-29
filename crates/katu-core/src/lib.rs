//! `katu-core` — E04: o kernel do katu.
//!
//! Máquina de estados, log append-only e a porta [`memory::Memory`] (DF1, DF6).

#![forbid(unsafe_code)]

pub mod memory;

/// Versão do vocabulário de política que o kernel respeita (E02).
pub use katu_policy::POLICY_VOCAB_VERSION;
