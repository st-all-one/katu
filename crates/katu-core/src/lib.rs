//! `katu-core` — E04: o kernel do katu.
//!
//! Máquina de estados, log append-only, a porta [`memory::Memory`] (DF1, DF6), os ports
//! determinísticos (E01-T02) e o diagnóstico estruturado transversal ([`diag`], DF9).

#![forbid(unsafe_code)]

pub mod diag;
pub mod memory;
pub mod ports;

/// Versão do vocabulário de política que o kernel respeita (E02).
pub use katu_policy::POLICY_VOCAB_VERSION;
