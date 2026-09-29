//! Kernel do katu (E04): **estado como valor**, log append-only, projeções puras.
//!
//! - `step(State, Event) -> Result<State, Refusal>` — a transição é uma função pura, sem
//!   singletons de processo (DF1).
//! - O log é a **fonte da verdade**: `Model-visible ⟺ logged` (§42). [`derive_messages`] projeta o
//!   histórico do modelo a partir dos eventos; [`state_of`] reproduz o estado.
//! - Tentativas falhadas ficam no log mas **não** acrescentam histórico ao modelo.

pub mod budget;
pub mod checkpoint;
mod event;
mod log;
mod pipeline;
mod project;
mod session;
mod state;
mod step;

pub use budget::{Budget, BudgetCap, BudgetGate, BudgetRefusal, Charge};
pub use checkpoint::{CHECKPOINT_SCHEMA_VERSION, Checkpoint, CheckpointError, checkpoint_path};
pub use event::{CallId, Event};
pub use log::{
    LOG_SCHEMA_VERSION, Log, LogError, LogErrorKind, LogRecord, read_records, session_path,
};
pub use pipeline::{Dispatch, Effect, Tool, dispatch, facts_for};
pub use project::{Message, Snapshot, derive_messages, snapshot, state_of};
pub use session::{CallContext, Session, SessionError};
pub use state::{CallStatus, Refusal, RefusalReason, State, can_transition, next_phase};
pub use step::step;
