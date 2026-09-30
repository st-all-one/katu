//! `katu-tui` — E10: interface de terminal.
//!
//! UX de codificação (`ratatui` + `crossterm`), panic-safe e com evidência no ecrã.
//!
//! Arquitetura (E10-T02): **estado central, render puro**. Tudo vive em [`App`]; [`map_key`]
//! traduz teclas em [`Action`] puras e [`App::apply_action`] muda só o estado, devolvendo um
//! [`Command`] quando a borda tem de agir. O I/O de terminal vive em [`run`], com restauro RAII e
//! panic hook (E10-T01). A borda implementa [`Handler`] para correr o loop de turnos — a UI nunca
//! fala com o provider.

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "módulos internos usam pub(crate); a API pública é a reexportação em `lib.rs`"
)]

mod action;
mod app;
mod approval;
mod controls;
mod entry;
mod layout;
mod live;
mod message;
mod run;
mod throttle;
mod trash;
mod ui;

pub use action::{Action, Mode, map_key};
pub use app::App;
pub use approval::{Challenge, ChallengePrompt, ChallengeSignature, QUESTIONS};
pub use entry::{Entry, Role, Status};
pub use live::Live;
pub use message::{Command, Update};
pub use run::{Handler, Painter, run};
pub use trash::TrashEntry;
pub use ui::render;
