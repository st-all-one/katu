//! Kernel do katu (E04): **estado como valor**, log append-only, projeções puras.
//!
//! - `step(State, Event) -> Result<State, Refusal>` — a transição é uma função pura, sem
//!   singletons de processo (DF1).
//! - O log é a **fonte da verdade**: `Model-visible ⟺ logged` (§42). [`derive_messages`] projeta o
//!   histórico do modelo a partir dos eventos; [`state_of`] reproduz o estado.
//! - Tentativas falhadas ficam no log mas **não** acrescentam histórico ao modelo.

pub mod budget;
pub mod bus;
pub mod checkpoint;
pub mod cost;
mod event;
mod log;
mod memory_gate;
mod pipeline;
mod project;
mod session;
mod state;
mod step;

pub use budget::{Budget, BudgetCap, BudgetGate, BudgetRefusal, Charge};
pub use bus::{EventBus, HandlerError, HandlerResult, Middleware, Next, Observer};
pub use checkpoint::{CHECKPOINT_SCHEMA_VERSION, Checkpoint, CheckpointError, checkpoint_path};
pub use cost::{
    CostCaps, CostCharge, CostGovernor, CostLayer, CostRefusal, KillSwitch, Reenable,
    ReenableError, RollingWindowCap, VelocityCap, cost_charge_for,
};
pub use event::{CallId, Event};
pub use log::{
    LOG_SCHEMA_VERSION, Log, LogError, LogErrorKind, LogRecord, read_records, session_path,
};
pub use memory_gate::{
    MemoryWriteError, MemoryWriteRequest, enforce_memory_write, memory_recall_use, memory_write_use,
};
pub use pipeline::{
    Dispatch, DispatchRequest, Effect, Tool, ToolOutput, dispatch, dispatch_with, facts_for,
    facts_from,
};
pub use project::{Message, Snapshot, derive_messages, snapshot, state_of};
pub use session::{
    CallContext, Session, SessionError, SessionId, SessionMeta, StateSnapshot, audit_dir,
    discover_root, katu_dir,
};
pub use state::{CallStatus, Refusal, RefusalReason, State, can_transition, next_phase};
pub use step::step;
