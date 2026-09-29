//! `katu-tools` — E06/E07: ferramentas, capacidades e contenção soft.
//!
//! Superfície de tools do core (write/read/edit/trash/exec/search + planning) e a contenção
//! determinística **soft** (fail-closed): controlo em falta = recusa. Nunca fala com providers
//! (firewall LLM-free, DF8).

#![forbid(unsafe_code)]

pub mod read;
pub mod write;
