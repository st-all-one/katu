//! Testes do armazenamento de auditoria (selagem, manifesto, consulta).

use crate::kernel::Visibility;
use std::path::Path;

use super::{AuditStore, Query};
use crate::kernel::Event;
use crate::kernel::audit_dir;
use crate::ports::{Fs, MemFs};

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
                visibility: Visibility::User,
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
            store.append_event(
                i.saturating_add(1),
                &Event::UserMessage {
                    text,
                    visibility: Visibility::User,
                },
            )?;
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

#[test]
fn verify_detects_tampered_segment() -> Result<(), Box<dyn std::error::Error>> {
    // D2: a cadeia de hash deteta adulteração do conteúdo de um segmento.
    let fs = MemFs::new();
    let root = Path::new("/work/tamper");
    {
        let mut store = AuditStore::open(&fs, root)?;
        store.append_event(
            1,
            &Event::UserMessage {
                text: "mensagem original".to_string(),
                visibility: Visibility::User,
            },
        )?;
        store.flush()?;
    }
    // A cadeia está intacta.
    let store = AuditStore::open(&fs, root)?;
    store.verify()?;

    // Adultera o conteúdo do segmento.
    let rec_path = audit_dir(root).join("seg-000001.rec");
    let mut content = String::from_utf8(fs.read(&rec_path)?).map_err(|e| e.to_string())?;
    content.push_str("\nMENSAGEM_ADULTERADA");
    fs.write_atomic(&rec_path, content.as_bytes())?;

    // A verificação deteta a adulteração.
    let store = AuditStore::open(&fs, root)?;
    let result = store.verify();
    assert!(result.is_err(), "a adulteração tem de ser detetada");
    Ok(())
}

#[test]
fn verify_detects_removed_segment() -> Result<(), Box<dyn std::error::Error>> {
    // D2: a cadeia de hash deteta a remoção de um segmento.
    let fs = MemFs::new();
    let root = Path::new("/work/removed");
    {
        let mut store = AuditStore::open(&fs, root)?;
        for i in 0..300_u64 {
            store.append_event(
                i.saturating_add(1),
                &Event::UserMessage {
                    text: format!("evento {i}"),
                    visibility: Visibility::User,
                },
            )?;
        }
        store.flush()?;
    }
    // Remove o primeiro segmento.
    fs.remove(&audit_dir(root).join("seg-000001.rec"))?;

    // A verificação deteta a remoção (o segmento não pode ser lido).
    let store = AuditStore::open(&fs, root)?;
    assert!(
        store.verify().is_err(),
        "a remoção de um segmento tem de ser detetada"
    );
    Ok(())
}

/// A/B determinístico da deteção de adulteração (D2): escreve o artefacto em `KATU_AUDIT_OUT`.
#[test]
#[ignore = "bench A/B: escreve o artefacto do protocolo (a via normal é o gate)"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_audit_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let root = Path::new("/work/audit-artifact");
    {
        let mut store = AuditStore::open(&fs, root)?;
        for i in 0..300_u64 {
            store.append_event(
                i.saturating_add(1),
                &Event::UserMessage {
                    text: format!("evento {i}"),
                    visibility: Visibility::User,
                },
            )?;
        }
        store.flush()?;
    }
    let store = AuditStore::open(&fs, root)?;
    let segments = store.manifest().segments.len();
    let intact = store.verify().is_ok();

    // Adultera o primeiro segmento.
    let rec_path = audit_dir(root).join("seg-000001.rec");
    let mut content = String::from_utf8(fs.read(&rec_path)?).map_err(|e| e.to_string())?;
    content.push_str("\nADULTERADO");
    fs.write_atomic(&rec_path, content.as_bytes())?;
    let store = AuditStore::open(&fs, root)?;
    let tamper_detected = store.verify().is_err();

    let value = serde_json::json!({
        "schema": "katu.bench.audit.v1",
        "question": "a cadeia de hash da auditoria deteta adulteração",
        "rule": "cada segmento tem um hash do conteúdo e um ponteiro ao hash anterior; qualquer adulteração quebra a cadeia",
        "scenarios": [
            { "name": "intact_chain", "segments": segments, "verify_passes": intact },
            { "name": "tamper_detected", "verify_fails": tamper_detected },
        ],
        "totals": { "segments": segments, "intact": intact, "tamper_detected": tamper_detected },
        "criterion": "a cadeia está intacta e a adulteração é detetada",
        "criterion_met": intact && tamper_detected,
        "caveat": "proxy determinístico (sem I/O real): mede a deteção de adulteração, não a segurança criptográfica do hash (FNV-1a não é um MAC)",
        "decision": "default on (a cadeia de hash é o mecanismo de deteção de adulteração)",
    });
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_AUDIT_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed.get("criterion_met"), Some(&serde_json::json!(true)));
    Ok(())
}
