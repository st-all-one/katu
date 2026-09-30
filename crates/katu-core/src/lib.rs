//! `katu-core` — E04: o kernel do katu.
//!
//! Máquina de estados, log append-only, a porta [`memory::Memory`] (DF1, DF6), os ports
//! determinísticos (E01-T02), o modelo de erro ([`error`], E01-T06) e o diagnóstico estruturado
//! transversal ([`diag`], DF9).

#![forbid(unsafe_code)]

pub mod audit;
pub mod containment;
pub mod context;
pub mod diag;
pub mod error;
pub mod evidence;
pub mod feedback;
pub mod kernel;
pub mod memory;
pub mod model;
pub mod plan;
pub mod ports;
pub mod provider;
pub mod report;
pub mod toon;
pub mod validate;
pub mod verify;

/// Versão do vocabulário de política que o kernel respeita (E02).
pub use katu_policy::POLICY_VOCAB_VERSION;
