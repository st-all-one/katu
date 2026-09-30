//! Auditoria densa e pesquisável (ADR 0009).
//!
//! Histórico completo em `.katu/audit`, **local** e nunca versionado. Segmentos colunares imutáveis
//! e um índice invertido derivado (termos, frases, filtros de campo). A compactação não toca aqui:
//! o original permanece endereçável/pesquisável.

mod codec;
mod index;
mod record;
mod store;

pub use index::{Group, Index, Posting, Query};
pub use record::{AuditRecord, MAX_TEXT_BYTES};
pub use store::{
    AUDIT_SCHEMA_VERSION, AuditError, AuditStore, Hit, Manifest, SEGMENT_EVENTS, SegmentInfo,
};
