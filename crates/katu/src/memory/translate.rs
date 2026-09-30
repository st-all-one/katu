//! Tradução de tipos katu ↔ knudge (E03-T02) — o único sítio com o vocabulário do knudge.

use katu_core::memory::{
    Basis, MemoryError, NoteRef, NoteType, PreEditOutcome, PreWriteOutcome, RecallHit, Score,
    Status,
};
use knudge_core::retrieval::RecallHit as KnudgeHit;
use knudge_core::schema::NoteType as KnudgeNoteType;
use knudge_core::schema::Status as KnudgeStatus;
use knudge_core::schema::id::note_id;
use knudge_core::write::{DedupDecision, Draft};

/// Converte o tipo de nota do katu no do knudge.
pub(crate) fn note_type(note_type: NoteType) -> KnudgeNoteType {
    let _span = katu_core::trace_fn!("memory::translate::note_type");

    match note_type {
        NoteType::Decision => KnudgeNoteType::Decision,
        NoteType::Question => KnudgeNoteType::Question,
        NoteType::Task => KnudgeNoteType::Task,
        NoteType::Def => KnudgeNoteType::Def,
        NoteType::Error => KnudgeNoteType::Error,
        NoteType::Snippet => KnudgeNoteType::Snippet,
        NoteType::Link => KnudgeNoteType::Link,
        NoteType::Meta => KnudgeNoteType::Meta,
        NoteType::Risk => KnudgeNoteType::Risk,
        _ => KnudgeNoteType::Fact,
    }
}

/// Converte o tipo de nota do knudge no do katu.
pub(crate) fn from_note_type(note_type: KnudgeNoteType) -> NoteType {
    let _span = katu_core::trace_fn!("memory::translate::from_note_type");

    match note_type {
        KnudgeNoteType::Fact => NoteType::Fact,
        KnudgeNoteType::Decision => NoteType::Decision,
        KnudgeNoteType::Question => NoteType::Question,
        KnudgeNoteType::Task | KnudgeNoteType::Epic => NoteType::Task,
        KnudgeNoteType::Def => NoteType::Def,
        KnudgeNoteType::Error => NoteType::Error,
        KnudgeNoteType::Snippet => NoteType::Snippet,
        KnudgeNoteType::Link => NoteType::Link,
        KnudgeNoteType::Meta => NoteType::Meta,
        KnudgeNoteType::Risk => NoteType::Risk,
    }
}

/// Converte o estado do knudge no do katu.
pub(crate) fn from_status(status: KnudgeStatus) -> Status {
    let _span = katu_core::trace_fn!("memory::translate::from_status");

    match status {
        KnudgeStatus::Active => Status::Active,
        KnudgeStatus::InProgress => Status::InProgress,
        KnudgeStatus::Blocked => Status::Blocked,
        KnudgeStatus::Closed => Status::Closed,
        KnudgeStatus::Superseded => Status::Superseded,
        KnudgeStatus::Forgotten => Status::Forgotten,
    }
}

/// Converte um pedido de escrita no rascunho do knudge.
pub(crate) fn draft(statement: &str, kind: NoteType, body: &str, anchor: Option<&str>) -> Draft {
    let _span = katu_core::trace_fn!("memory::translate::draft");

    let mut draft = Draft::new(note_type(kind), statement.to_string());
    if !body.is_empty() {
        draft.body = body.to_string();
    }
    if let Some(anchor) = anchor {
        draft.anchors = vec![anchor.to_string()];
    }
    draft
}

/// Decide o `pre_edit` em *dry-run*, espelhando a regra de `write::update` (D01/D48).
///
/// Muda a **chave de conteúdo** (`type` + `statement`) → `Supersede` (novo `id`); caso contrário,
/// revisa no lugar. Ids não-deriváveis (históricos) revisam no lugar quando a afirmação não muda.
pub(crate) fn edit_outcome(
    id: &str,
    note_type: KnudgeNoteType,
    original_statement: &str,
    new_statement: &str,
) -> PreEditOutcome {
    let _span = katu_core::trace_fn!("memory::translate::edit_outcome");

    let canonical_id = note_id(note_type, original_statement);
    let new_id = note_id(note_type, new_statement);
    let content_key_changed = new_statement != original_statement;
    let revises_in_place = new_id == id || (canonical_id != id && !content_key_changed);
    if revises_in_place {
        PreEditOutcome::Update
    } else {
        PreEditOutcome::Supersede {
            target: NoteRef::new(new_id),
        }
    }
}

/// Converte a decisão de dedup do knudge no resultado de `pre_write`.
///
/// # Errors
/// [`MemoryError::internal`] se a similaridade não couber nos pontos base (não deve acontecer).
pub(crate) fn outcome(decision: DedupDecision) -> Result<PreWriteOutcome, MemoryError> {
    let _span = katu_core::trace_fn!("memory::translate::outcome");

    Ok(match decision {
        DedupDecision::Create => PreWriteOutcome::Create,
        DedupDecision::Merge { candidate, score } => PreWriteOutcome::Merge {
            target: NoteRef::new(candidate),
            score: points(score)?,
            basis: Basis::Inferred,
        },
        DedupDecision::Reject { candidate, score } => PreWriteOutcome::Reject {
            duplicate: NoteRef::new(candidate),
            score: points(score)?,
            basis: Basis::Inferred,
        },
    })
}

/// Converte um hit do `recall` do knudge no hit da porta.
///
/// # Errors
/// [`MemoryError::internal`] se a confiança não couber nos pontos base.
pub(crate) fn hit(hit: &KnudgeHit) -> Result<RecallHit, MemoryError> {
    let _span = katu_core::trace_fn!("memory::translate::hit");

    Ok(RecallHit {
        note: NoteRef::new(hit.id.clone()),
        statement: hit.statement.clone(),
        score: points(hit.confidence)?,
        basis: Basis::Inferred,
        anchor: None,
    })
}

/// Confiança `[0,1]` do knudge → pontos base do katu.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "confidence ∈ [0,1] × 10000 cabe em u16; conversão documentada na borda do adaptador"
)]
pub(crate) fn points(confidence: f64) -> Result<Score, MemoryError> {
    let _span = katu_core::trace_fn!("memory::translate::points");

    let clamped = confidence.clamp(0.0, 1.0);
    let scaled = (clamped * f64::from(Score::MAX_BASIS_POINTS)).round() as i64;
    let raw = u16::try_from(scaled).unwrap_or(Score::MAX_BASIS_POINTS);
    Score::from_basis_points(raw)
        .ok_or_else(|| MemoryError::internal(format!("score fora do intervalo: {raw}")))
}
