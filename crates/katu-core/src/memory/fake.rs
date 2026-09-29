//! `FakeMemory` — cenários fixos para os testes do kernel (E03-T05), sem puxar o `knudge-core`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Memory;
use super::error::{MemoryError, MemoryErrorKind};
use super::io::{
    MemoryStatus, PreEditOutcome, PreEditReq, PreWriteOutcome, PreWriteReq, RecallHit, RecallReq,
    SessionEndOutcome, SessionEndReq,
};
use super::types::{Basis, NoteRef, Score};

/// Memória de teste com respostas fixas e falha injetável.
#[derive(Debug, Clone)]
pub struct FakeMemory {
    /// Resposta de `pre_write`.
    pub pre_write: PreWriteOutcome,
    /// Resposta de `pre_edit`.
    pub pre_edit: PreEditOutcome,
    /// Resposta de `session_end`.
    pub session_end: SessionEndOutcome,
    /// Resposta de `status`.
    pub status: MemoryStatus,
    /// Resultados fixos de `search` (recall).
    pub recall: Vec<RecallHit>,
    /// Quando `Some`, todos os métodos falham com esta natureza.
    pub fail: Option<MemoryErrorKind>,
    /// Número de `record` efetivos (prova de "sem efeito" quando a política nega).
    recorded: Arc<AtomicUsize>,
}

impl Default for FakeMemory {
    fn default() -> Self {
        Self {
            pre_write: PreWriteOutcome::Create,
            pre_edit: PreEditOutcome::Update,
            session_end: SessionEndOutcome::default(),
            status: MemoryStatus {
                backend: "fake".to_string(),
                ..MemoryStatus::default()
            },
            recall: Vec::new(),
            fail: None,
            recorded: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl FakeMemory {
    /// Fake que rejeita todas as escritas como duplicata forte.
    #[must_use]
    pub fn rejecting(duplicate: NoteRef, score: Score) -> Self {
        Self {
            pre_write: PreWriteOutcome::Reject {
                duplicate,
                score,
                basis: Basis::Measured,
            },
            ..Self::default()
        }
    }

    /// Fake que falha sempre com a natureza dada.
    #[must_use]
    pub fn failing(kind: MemoryErrorKind) -> Self {
        Self {
            fail: Some(kind),
            ..Self::default()
        }
    }

    /// Fake que devolve resultados fixos no `search`.
    #[must_use]
    pub fn with_hits(hits: Vec<RecallHit>) -> Self {
        Self {
            recall: hits,
            ..Self::default()
        }
    }

    /// Falha injetada, se houver.
    fn failure(&self) -> Option<MemoryError> {
        self.fail
            .map(|kind| MemoryError::new(kind, "falha injetada no FakeMemory"))
    }

    /// Número de commits (`record`) efetivos.
    #[must_use]
    pub fn recorded(&self) -> usize {
        self.recorded.load(Ordering::SeqCst)
    }
}

impl Memory for FakeMemory {
    fn pre_write(&self, _req: &PreWriteReq) -> Result<PreWriteOutcome, MemoryError> {
        match self.failure() {
            Some(err) => Err(err),
            None => Ok(self.pre_write.clone()),
        }
    }

    fn pre_edit(&self, _req: &PreEditReq) -> Result<PreEditOutcome, MemoryError> {
        match self.failure() {
            Some(err) => Err(err),
            None => Ok(self.pre_edit.clone()),
        }
    }

    fn record(&self, _req: &PreWriteReq) -> Result<NoteRef, MemoryError> {
        if let Some(err) = self.failure() {
            return Err(err);
        }
        self.recorded.fetch_add(1, Ordering::SeqCst);
        Ok(NoteRef::new("note:recorded"))
    }

    fn search(&self, req: &RecallReq) -> Result<Vec<RecallHit>, MemoryError> {
        if let Some(err) = self.failure() {
            return Err(err);
        }
        Ok(self.recall.iter().take(req.limit).cloned().collect())
    }

    fn session_end(&self, _req: &SessionEndReq) -> Result<SessionEndOutcome, MemoryError> {
        match self.failure() {
            Some(err) => Err(err),
            None => Ok(self.session_end.clone()),
        }
    }

    fn status(&self) -> Result<MemoryStatus, MemoryError> {
        match self.failure() {
            Some(err) => Err(err),
            None => Ok(self.status.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FakeMemory;
    use crate::memory::{
        Basis, Health, Memory, MemoryError, MemoryErrorKind, MemoryStatus, NoteRef, NoteType,
        PreWriteOutcome, PreWriteReq, RecallHit, RecallReq, Score, Status,
    };

    fn request() -> PreWriteReq {
        PreWriteReq {
            statement: "cache usa LRU".to_string(),
            note_type: NoteType::Decision,
            anchor: None,
            body: String::new(),
        }
    }

    #[test]
    fn default_fake_creates() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::default();
        assert_eq!(memory.pre_write(&request())?, PreWriteOutcome::Create);
        assert_eq!(memory.status()?.health, Health::Healthy);
        Ok(())
    }

    #[test]
    fn rejecting_fake_reports_duplicate() -> Result<(), Box<dyn std::error::Error>> {
        let score = Score::from_basis_points(9_500).ok_or("score inválido")?;
        let memory = FakeMemory::rejecting(NoteRef::new("fact_1"), score);
        let outcome = memory.pre_write(&request())?;
        assert!(matches!(
            outcome,
            PreWriteOutcome::Reject {
                basis: Basis::Measured,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn failing_fake_is_only_retryable_on_timeout() {
        let timeout = FakeMemory::failing(MemoryErrorKind::Timeout);
        assert!(
            timeout
                .status()
                .err()
                .as_ref()
                .is_some_and(MemoryError::retryable)
        );
        let internal = FakeMemory::failing(MemoryErrorKind::Internal);
        assert!(
            internal
                .status()
                .err()
                .as_ref()
                .is_some_and(|e| !e.retryable())
        );
    }

    #[test]
    fn record_counts_commits() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::default();
        assert_eq!(memory.recorded(), 0);
        memory.record(&request())?;
        assert_eq!(memory.recorded(), 1);
        Ok(())
    }

    #[test]
    fn failing_record_is_not_counted() {
        let memory = FakeMemory::failing(MemoryErrorKind::Internal);
        assert!(memory.record(&request()).is_err());
        assert_eq!(memory.recorded(), 0);
    }

    #[test]
    fn search_returns_fixed_hits_bounded_by_limit() -> Result<(), Box<dyn std::error::Error>> {
        let score = Score::from_basis_points(9_000).ok_or("score inválido")?;
        let hit = RecallHit {
            note: NoteRef::new("n1"),
            statement: "cache usa LRU".to_string(),
            score,
            basis: Basis::Measured,
            anchor: None,
        };
        let memory = FakeMemory::with_hits(vec![hit.clone(), hit.clone()]);
        let hits = memory.search(&RecallReq {
            query: "cache".to_string(),
            limit: 1,
        })?;
        assert_eq!(hits.len(), 1, "o limite é respeitado");
        assert_eq!(hits.first(), Some(&hit));
        Ok(())
    }

    #[test]
    fn status_default_is_healthy() {
        assert_eq!(MemoryStatus::default().health, Health::Healthy);
        assert_eq!(Status::Active, Status::Active);
    }
}
