//! Protocolo do kernel (`KERNEL_SURFACE` F0): a fronteira entre o kernel e as superfícies.
//!
//! O kernel é autocontido e vive na sua thread; as superfícies (CLI, TUI e front-ends futuros)
//! comunicam **só** por este protocolo: enviam [`Command`] e consomem [`Event`]. Os tipos são
//! `serde`-prontos — a thread pode tornar-se processo (F4) sem reescrever a fronteira — e **não**
//! carregam `Runtime`, `Session` nem `Provider` (K2/K7).
//!
//! Plano: `wiki/_ref/plan/KERNEL_SURFACE.md`.

mod command;
mod event;
mod handle;
mod live;
mod trash;

pub use command::{Command, LoginRequest};
pub use event::{ApprovalGrant, ApprovalRequest, Event, TurnSummary};
pub use handle::{
    COMMAND_QUEUE, KernelBus, KernelHandle, Publisher, RecvError, SendError, channel,
};
pub use live::Live;
pub use trash::TrashEntry;
