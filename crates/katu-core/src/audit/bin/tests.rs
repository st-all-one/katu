//! Testes do codec binário do índice (round-trip e rejeição de formato alheio).

use super::{decode, encode};
use crate::audit::bloom::Bloom;
use crate::audit::index::Index;
use crate::audit::record::AuditRecord;
use crate::kernel::Event;

/// Linhas de auditoria sintéticas com termos partilhados.
fn records() -> Vec<AuditRecord> {
    (0_u64..8)
        .map(|i| {
            AuditRecord::from_event(
                i,
                &Event::UserMessage {
                    text: format!("linha {i} parser de toon"),
                },
            )
        })
        .collect()
}

#[test]
fn round_trips_index_and_bloom() -> Result<(), Box<dyn std::error::Error>> {
    let index = Index::build(&records());
    let bloom = Bloom::from_terms(index.postings().keys().map(String::as_str));
    let bytes = encode(&index, &bloom);
    let (decoded, decoded_bloom) = decode(&bytes).ok_or("decode devia reconhecer o formato")?;
    assert_eq!(decoded, index, "o índice deve sobreviver ao codec");
    assert!(decoded_bloom.might_contain("parser"));
    assert!(!decoded_bloom.might_contain("zzz_inexistente_999"));
    Ok(())
}

#[test]
fn rejects_foreign_bytes() {
    assert!(decode(b"nope").is_none());
    assert!(decode(b"").is_none());
}
