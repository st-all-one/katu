//! Adaptador in-process da porta `Memory` sobre o `knudge-core` (E03-T02).
//!
//! É o **único** sítio do katu com o vocabulário do knudge (firewall `check-layers`). A fachada
//! [`Knudge`] é `!Sync`, pelo que é protegida por um `Mutex`; o índice/grafo (caros) ficam em
//! cache e são invalidados por cada escrita. O core do knudge é síncrono — o caminho async
//! envolve-o com `spawn_blocking` + timeout (E03-T04).

pub(crate) mod commands;
mod drain;
mod translate;

pub(crate) use drain::DrainSummary;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use katu_core::diag::{Level, events};
use katu_core::memory::{
    Anchor, Health, Memory, MemoryError, MemoryErrorKind, MemoryStatus, NoteRef, PreEditOutcome,
    PreEditReq, PreWriteOutcome, PreWriteReq, RecallHit, RecallReq, SessionEndOutcome,
    SessionEndReq,
};
use knudge_core::graph::Graph;
use knudge_core::retrieval::{Index, RecallQuery, recall};
use knudge_core::write::{Draft, propose, write};
use knudge_core::{Error as KnudgeError, ErrorKind, Knudge};

/// Layout do conhecimento dentro de `.katu/` (E20-T19): `notas/` + `.idx/`.
const KNOWLEDGE_LAYOUT: &str = ".katu/knowledge";

/// Estado com cache do índice/grafo (invalidado a cada commit).
struct Inner {
    kd: Knudge,
    strict: bool,
    index: Option<Index>,
    graph: Option<Graph>,
}

impl Inner {
    fn ensure_index(&mut self) -> Result<(), MemoryError> {
        if self.index.is_none() {
            self.index = Some(self.kd.index().map_err(to_memory_error)?);
        }
        Ok(())
    }

    fn ensure_graph(&mut self) -> Result<(), MemoryError> {
        if self.graph.is_none() {
            self.graph = Some(self.kd.graph().map_err(to_memory_error)?);
        }
        Ok(())
    }
}

/// Memória do agente servida pelo núcleo puro do knudge, in-process.
pub(crate) struct KnudgeMemory {
    inner: Mutex<Inner>,
}

impl KnudgeMemory {
    /// Abre o projeto em `root` com o layout de conhecimento default (`.knudge`).
    ///
    /// # Errors
    /// [`MemoryError`] se a resolução do projeto/config ou o acesso ao disco falharem.
    pub(crate) fn open(root: &Path) -> Result<Self, MemoryError> {
        let kd = Knudge::builder()
            .root(root)
            .knowledge_dir(KNOWLEDGE_LAYOUT)
            .open()
            .map_err(to_memory_error)?;
        // `behavior.strict` fixado na abertura: avisos *soft* do knudge promovem a erro.
        let strict = kd.config().get_bool("behavior.strict").unwrap_or(false);
        Ok(Self {
            inner: Mutex::new(Inner {
                kd,
                strict,
                index: None,
                graph: None,
            }),
        })
    }

    /// Diretório de conhecimento (para diagnóstico).
    pub(crate) fn knowledge_dir(&self) -> PathBuf {
        lock(&self.inner).kd.knowledge_dir()
    }

    /// Drena a fila de embeddings do projeto (E20-T20).
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "`force` é o modo `--force` do dreno"
    )]
    pub(crate) fn drain(&self, force: bool) -> Result<DrainSummary, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_WRITE, "op" => "drain");
        let inner = lock(&self.inner);
        drain::run(&inner.kd, force)
    }
}

impl Memory for KnudgeMemory {
    fn pre_write(&self, req: &PreWriteReq) -> Result<PreWriteOutcome, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_WRITE, "op" => "pre_write");
        let mut inner = lock(&self.inner);
        let draft = make_draft(req);
        let thresholds = inner.kd.thresholds().map_err(to_memory_error)?;
        inner.ensure_index()?;
        let proposal = {
            let index = inner
                .index
                .as_ref()
                .ok_or_else(|| MemoryError::internal("índice ausente após construção"))?;
            propose(index, &draft, &thresholds).map_err(to_memory_error)?
        };
        translate::outcome(proposal.decision)
    }

    fn pre_edit(&self, req: &PreEditReq) -> Result<PreEditOutcome, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_WRITE, "op" => "pre_edit");
        let inner = lock(&self.inner);
        let note = match inner.kd.store().read(req.note.as_str()) {
            Ok(note) => note,
            Err(KnudgeError::NotFound(_)) => {
                return Ok(PreEditOutcome::Reject {
                    reason: format!("nota ausente: {}", req.note.as_str()),
                });
            }
            Err(error) => return Err(to_memory_error(error)),
        };
        let note_type = note.frontmatter.note_type().map_err(to_memory_error)?;
        let original = note.frontmatter.statement().map_err(to_memory_error)?;
        Ok(translate::edit_outcome(
            req.note.as_str(),
            note_type,
            original,
            &req.statement,
        ))
    }

    fn record(&self, req: &PreWriteReq) -> Result<NoteRef, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_WRITE, "op" => "record");
        let mut inner = lock(&self.inner);
        let draft = make_draft(req);
        let id = {
            let thresholds = inner.kd.thresholds().map_err(to_memory_error)?;
            let ctx = inner.kd.write_context().map_err(to_memory_error)?;
            write(&ctx, &draft, &thresholds)
                .map_err(to_memory_error)?
                .id
        };
        inner.index = None;
        inner.graph = None;
        Ok(NoteRef::new(id))
    }

    fn search(&self, req: &RecallReq) -> Result<Vec<RecallHit>, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_RECALL, "limit" => req.limit);
        let mut inner = lock(&self.inner);
        inner.ensure_index()?;
        inner.ensure_graph()?;
        let (Some(index), Some(graph)) = (inner.index.as_ref(), inner.graph.as_ref()) else {
            return Err(MemoryError::internal(
                "índice/grafo ausentes após construção",
            ));
        };
        let mut query = RecallQuery::new(req.query.clone());
        query.limit = req.limit;
        query.strict = inner.strict;
        let output = recall(index, graph, &query).map_err(to_memory_error)?;
        output.hits.iter().map(translate::hit).collect()
    }

    fn session_end(&self, req: &SessionEndReq) -> Result<SessionEndOutcome, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_HANDOFF);
        let mut warnings = Vec::new();
        if req.task.is_some() {
            warnings.push(
                "fecho de tarefa exige evidência (health::close_task), não session_end".to_string(),
            );
        }
        Ok(SessionEndOutcome {
            committed: Vec::new(),
            warnings,
        })
    }

    fn status(&self) -> Result<MemoryStatus, MemoryError> {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_STATUS);
        let inner = lock(&self.inner);
        let dir = inner.kd.knowledge_dir();
        let mut warnings = Vec::new();
        if !dir.exists() {
            warnings.push(format!(
                "diretório de conhecimento ainda ausente: {}",
                dir.display()
            ));
        }
        Ok(MemoryStatus {
            backend: "knudge-in-process".to_string(),
            health: Health::Healthy,
            warnings,
        })
    }
}

/// Constrói o rascunho do knudge a partir do pedido do katu.
fn make_draft(req: &PreWriteReq) -> Draft {
    translate::draft(
        &req.statement,
        req.note_type,
        &req.body,
        req.anchor.as_ref().map(Anchor::as_str),
    )
}

/// Mapeia o erro do knudge na taxonomia da porta (preserva `Timeout` como retentável).
#[allow(
    clippy::needless_pass_by_value,
    reason = "`map_err(to_memory_error)` passa o erro por valor e consome-o na mensagem"
)]
fn to_memory_error(error: KnudgeError) -> MemoryError {
    let kind = match error.kind() {
        ErrorKind::Timeout => MemoryErrorKind::Timeout,
        ErrorKind::InvalidInput | ErrorKind::Schema | ErrorKind::Config | ErrorKind::NotFound => {
            MemoryErrorKind::Invalid
        }
        ErrorKind::Io | ErrorKind::UnsafeBlocked => MemoryErrorKind::Unavailable,
        _ => MemoryErrorKind::Internal,
    };
    MemoryError::new(kind, error.to_string())
}

/// Bloqueia um `Mutex`, recuperando o valor mesmo que o lock esteja envenenado.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
