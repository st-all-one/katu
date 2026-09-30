//! Testes do armazenamento de auditoria (selagem, manifesto, consulta).

use std::path::Path;

use super::{AuditStore, Query};
use crate::kernel::Event;
use crate::ports::MemFs;

#[test]
fn seal_then_search_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    {
        let mut store = AuditStore::open(&fs, root)?;
        store.append_event(
            1,
            &Event::UserMessage {
                text: "corrige o parser de toon".to_string(),
            },
        )?;
        store.append_event(
            2,
            &Event::AssistantMessage {
                text: "vou corrigir o parser".to_string(),
            },
        )?;
        store.flush()?;
    }
    let store = AuditStore::open(&fs, root)?;
    assert_eq!(store.manifest().segments.len(), 1);
    let hits = store.search(&Query::parse("parser"), 10)?;
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|hit| hit.preview.contains("parser")));
    let phrase = store.search(&Query::parse("\"parser de toon\""), 10)?;
    assert_eq!(phrase.len(), 1);
    Ok(())
}

#[test]
fn searches_across_sealed_segments() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/seg");
    {
        let mut store = AuditStore::open(&fs, root)?;
        for i in 0..300_u64 {
            let text = if i < 256 {
                format!("alpha evento {i}")
            } else {
                format!("beta evento {i}")
            };
            store.append_event(i.saturating_add(1), &Event::UserMessage { text })?;
        }
        store.flush()?;
    }
    let store = AuditStore::open(&fs, root)?;
    assert_eq!(store.manifest().segments.len(), 2);
    // O segmento 2 não contém `alpha`: o Bloom descarta-o antes de ler o `.rec`.
    assert_eq!(store.search(&Query::parse("alpha"), 1000)?.len(), 256);
    assert_eq!(store.search(&Query::parse("beta"), 1000)?.len(), 44);
    Ok(())
}

#[test]
fn bloom_skips_segments_without_the_term() {
    use super::might_match;
    use crate::audit::bloom::Bloom;

    let bloom = Bloom::from_terms(["parser", "toon"].into_iter());
    assert!(might_match(&bloom, &Query::parse("parser")));
    assert!(!might_match(&bloom, &Query::parse("zzz_inexistente_999")));
    // Só filtros estruturais (sem termos): o Bloom não prova ausência, logo não descarta.
    assert!(might_match(&bloom, &Query::parse("kind:user")));
}
