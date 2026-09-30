//! Auditoria densa e pesquisável (ADR 0009).
//!
//! Histórico completo em `.katu/audit`, **local** e nunca versionado. Segmentos colunares imutáveis
//! e um índice invertido derivado (termos, frases, filtros de campo). A compactação não toca aqui:
//! o original permanece endereçável/pesquisável.

mod bin;
mod bloom;
mod codec;
mod index;
mod record;
mod store;

pub use index::{Group, Index, Posting, Query};
pub use record::{AuditRecord, MAX_TEXT_BYTES};
pub use store::{
    AUDIT_SCHEMA_VERSION, AuditError, AuditStore, Hit, Manifest, SEGMENT_EVENTS, SegmentInfo,
};

/// Codifica o índice no formato binário (delta+varint+Bloom) — ferramentas/testes (ADR 0009).
#[must_use]
pub fn encode_index(index: &Index) -> Vec<u8> {
    let _span = crate::trace_fn!("audit::encode_index");

    let bloom = bloom::Bloom::from_terms(index.postings().keys().map(String::as_str));
    bin::encode(index, &bloom)
}

/// Descarta o Bloom e devolve o índice (o armazenamento usa a via interna com Bloom).
#[must_use]
pub fn decode_index(bytes: &[u8]) -> Option<Index> {
    let _span = crate::trace_fn!("audit::decode_index");

    bin::decode(bytes).map(|(index, _)| index)
}
