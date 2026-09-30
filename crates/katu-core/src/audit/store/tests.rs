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
