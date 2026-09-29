//! `katu-tools` — E06/E07: ferramentas, capacidades e contenção soft.
//!
//! Superfície de tools do core (write/read/edit/trash/exec/search + planning) e a contenção
//! determinística **soft** (fail-closed): controlo em falta = recusa. Nunca fala com providers
//! (firewall LLM-free, DF8).

#![forbid(unsafe_code)]

pub mod edit;
pub mod lang;
pub mod move_file;
pub mod outline;
pub mod read;
pub mod registry;
pub mod search;
pub mod write;
pub mod write_file;
