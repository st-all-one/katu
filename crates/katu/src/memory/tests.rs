//! Testes do adaptador in-process (E03-T05): a suíte de conformidade corre contra o knudge.

use std::path::PathBuf;

use katu_core::memory::{
    Memory, NoteRef, NoteType, PreEditOutcome, PreEditReq, PreWriteReq, QueryMode, QueryOutcome,
    QueryReq, assert_contract,
};

use super::KnudgeMemory;

/// Raiz temporária única por teste.
fn root(label: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("katu-knudge-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[test]
fn in_process_adapter_satisfies_the_contract() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("contract")?;
    let memory = KnudgeMemory::open(&root)?;
    assert_contract(&memory)?;
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn pre_edit_on_a_missing_note_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("preedit")?;
    let memory = KnudgeMemory::open(&root)?;
    let outcome = memory.pre_edit(&PreEditReq {
        note: NoteRef::new("fact_inexistente"),
        statement: "afirmação".to_string(),
        anchor: None,
    })?;
    assert!(matches!(outcome, PreEditOutcome::Reject { .. }));
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn pre_edit_decides_update_or_supersede_in_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("edit-decision")?;
    let memory = KnudgeMemory::open(&root)?;
    let note = memory.record(&PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Fact,
        anchor: None,
        body: String::new(),
    })?;
    let same = memory.pre_edit(&PreEditReq {
        note: note.clone(),
        statement: "cache usa LRU".to_string(),
        anchor: None,
    })?;
    assert!(matches!(same, PreEditOutcome::Update));
    let changed = memory.pre_edit(&PreEditReq {
        note,
        statement: "cache usa FIFO".to_string(),
        anchor: None,
    })?;
    assert!(
        matches!(changed, PreEditOutcome::Supersede { .. }),
        "afirmação nova tem de superseder para um id derivado"
    );
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn status_names_the_in_process_backend() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("status")?;
    let memory = KnudgeMemory::open(&root)?;
    assert_eq!(memory.status()?.backend, "knudge-in-process");
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn knowledge_lives_under_katu_and_embeddings_are_off_without_url()
-> Result<(), Box<dyn std::error::Error>> {
    let root = root("drain")?;
    // Sem `embeddings.url` no projeto, o dreno fica `off` (E20-T17) — nunca inventa endpoint.
    std::fs::create_dir_all(root.join(".katu"))?;
    std::fs::write(root.join(".katu/katu.toml"), "[embeddings]\nurl = \"\"\n")?;
    let memory = KnudgeMemory::open(&root)?;
    assert!(
        memory.knowledge_dir().ends_with(".katu/knowledge"),
        "conhecimento em .katu/knowledge (E20-T19), não .knudge"
    );
    memory.record(&PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Fact,
        anchor: None,
        body: String::new(),
    })?;
    let summary = memory.drain(false)?;
    assert!(
        !summary.enabled,
        "sem URL, embeddings off (nunca inventado)"
    );
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn rich_query_returns_hits_tags_and_the_map() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("query")?;
    // Embeddings `off` (sem `embeddings.url`): a consulta não toca na rede (E20-T17).
    std::fs::create_dir_all(root.join(".katu"))?;
    std::fs::write(root.join(".katu/katu.toml"), "[embeddings]\nurl = \"\"\n")?;
    let memory = KnudgeMemory::open(&root)?;
    let note = memory.record(&PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Fact,
        anchor: None,
        body: "detalhe do corpo".to_string(),
    })?;

    let recall = memory.query(&QueryReq::recall("cache", 5))?;
    assert!(
        matches!(recall.outcome, QueryOutcome::Hits(ref hits) if !hits.is_empty()),
        "recall devolve hits"
    );

    let tags = memory.query(&QueryReq {
        mode: QueryMode::Tags,
        ..QueryReq::default()
    })?;
    assert!(matches!(tags.outcome, QueryOutcome::Tags(_)));

    let map = memory.query(&QueryReq {
        mode: QueryMode::Map,
        members: true,
        ..QueryReq::default()
    })?;
    assert!(
        matches!(map.outcome, QueryOutcome::Clusters(ref clusters) if !clusters.is_empty()),
        "o mapa agrupa por tipo"
    );

    let get = memory.query(&QueryReq {
        mode: QueryMode::Get,
        ids: vec![note.as_str().to_string()],
        full_content: true,
        ..QueryReq::default()
    })?;
    assert!(matches!(
        get.outcome,
        QueryOutcome::Hits(ref hits) if hits.first().is_some_and(|hit| hit.body.is_some())
    ));

    std::fs::remove_dir_all(&root)?;
    Ok(())
}
