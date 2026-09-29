//! Kernel do katu (E04): **estado como valor**, log append-only, projeções puras.
//!
//! - `step(State, Event) -> Result<State, Refusal>` — a transição é uma função pura, sem
//!   singletons de processo (DF1).
//! - O log é a **fonte da verdade**: `Model-visible ⟺ logged` (§42). [`derive_messages`] projeta o
//!   histórico do modelo a partir dos eventos; [`state_of`] reproduz o estado.
//! - Tentativas falhadas ficam no log mas **não** acrescentam histórico ao modelo.

mod event;
mod log;
mod project;
mod state;
mod step;

pub use event::{CallId, Event};
pub use log::{
    LOG_SCHEMA_VERSION, Log, LogError, LogErrorKind, LogRecord, read_records, session_path,
};
pub use project::{Message, Snapshot, derive_messages, snapshot, state_of};
pub use state::{CallStatus, Refusal, RefusalReason, State, can_transition, next_phase};
pub use step::step;
