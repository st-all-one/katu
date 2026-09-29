//! Porta `Memory` — tipos do katu (DF6).
//!
//! O contrato completo (`pre_write`, `pre_edit`, `session_end`, `status`) vive aqui, com tipos do
//! katu: **nenhum tipo do `knudge-core`** aparece nesta API. A implementação in-process vive no
//! binário (`katu/src/memory/`), isolada por `xtask check-layers`; o [`FakeMemory`] cobre os testes
//! do kernel (E03-T05) e a suíte [`assert_contract`] corre contra qualquer backend.

mod conformance;
mod error;
mod fake;
mod io;
mod types;

pub use conformance::assert_contract;
pub use error::{MemoryError, MemoryErrorKind};
pub use fake::FakeMemory;
pub use io::{
    Health, MemoryStatus, PreEditOutcome, PreEditReq, PreWriteOutcome, PreWriteReq, RecallHit,
    RecallReq, SessionEndOutcome, SessionEndReq,
};
pub use types::{Anchor, Basis, NoteRef, NoteType, Score, Status};

/// Porta de memória do agente (substituível: in-process agora, MCP/E08 depois).
///
/// Todos os métodos são síncronos e devolvem erro tipado; o caminho async do kernel envolve-os com
/// `spawn_blocking` + timeout (E03-T04).
pub trait Memory: Send + Sync {
    /// Pré-validação de escrita: decisão `create`/`merge`/`reject` (dedup ≥ 0.92).
    ///
    /// # Errors
    /// [`MemoryError`] se o backend falhar ou o pedido for inválido.
    fn pre_write(&self, req: &PreWriteReq) -> Result<PreWriteOutcome, MemoryError>;

    /// Pré-validação de edição de nota existente.
    ///
    /// # Errors
    /// [`MemoryError`] se o backend falhar ou o pedido for inválido.
    fn pre_edit(&self, req: &PreEditReq) -> Result<PreEditOutcome, MemoryError>;

    /// Persiste a nota pré-validada (commit **por nota**, OA8). O chamador só invoca isto depois
    /// de a política permitir a `memory_write` (E05-T01).
    ///
    /// # Errors
    /// [`MemoryError`] se o backend falhar.
    fn record(&self, req: &PreWriteReq) -> Result<NoteRef, MemoryError>;

    /// Consulta (recall): devolve as notas mais próximas da consulta, por ordem de score.
    ///
    /// # Errors
    /// [`MemoryError`] se o backend falhar ou o pedido for inválido.
    fn search(&self, req: &RecallReq) -> Result<Vec<RecallHit>, MemoryError>;

    /// Finaliza a sessão: commit/sync e inferência do `outcome`.
    ///
    /// # Errors
    /// [`MemoryError`] se o backend falhar.
    fn session_end(&self, req: &SessionEndReq) -> Result<SessionEndOutcome, MemoryError>;

    /// Estado do backend (para a UI e o arranque fail-closed, DF4).
    ///
    /// # Errors
    /// [`MemoryError`] se o backend não responder.
    fn status(&self) -> Result<MemoryStatus, MemoryError>;
}
