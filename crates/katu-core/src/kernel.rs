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
mod confidence;
mod control;
pub mod cost;
mod event;
mod guard;
mod hash;
mod log;
mod memory_gate;
mod pipeline;
mod project;
mod session;
mod state;
pub(crate) mod step;

pub use budget::{Budget, BudgetCap, BudgetGate, BudgetRefusal, Charge};
pub use bus::{EventBus, HandlerError, HandlerResult, Middleware, Next, Observer};
pub use checkpoint::{CHECKPOINT_SCHEMA_VERSION, Checkpoint, CheckpointError, checkpoint_path};
pub use confidence::{enforced_verdicts, enforced_verdicts_report, rule_trials, tool_trials};
pub use control::{Control, ControlError, ControlState};
pub use cost::{
    CostCaps, CostCharge, CostGovernor, CostLayer, CostRefusal, KillSwitch, Reenable,
    ReenableError, RollingWindowCap, VelocityCap, cost_charge_for,
};
pub use event::{CallId, Event, Visibility};
pub use guard::{Alarm, AlarmKind, Call, Fingerprint, Guard, GuardParams};
pub use hash::{canonical as canonical_hash, fnv1a};
pub use log::{
    Durability, LOG_SCHEMA_VERSION, Log, LogError, LogErrorKind, LogRecord, read_records,
    session_path,
};
pub use memory_gate::{
    MemoryWriteError, MemoryWriteRequest, enforce_memory_write, memory_recall_use, memory_write_use,
};
pub use pipeline::{
    Dispatch, DispatchRequest, Effect, Tool, ToolOutput, dispatch, dispatch_with, facts_for,
    facts_from,
};
pub use project::{
    Message, Snapshot, WireMessage, derive_messages, snapshot, state_of, wire_messages,
};
pub use session::{
    CallContext, MAX_TAIL_BYTES, Session, SessionError, SessionId, SessionMeta, StateSnapshot,
    audit_dir, discover_root, katu_dir,
};
pub use state::{Refusal, RefusalReason, State, can_transition, next_phase};
pub use step::step;
