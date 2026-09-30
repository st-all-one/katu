//! Armazenamento de auditoria (ADR 0009): manifesto, segmentos colunares e índice derivado.
//!
//! Segmentos selados são **imutáveis** (`seg-<NNNNNN>.rec` + `.idx`). O índice é **derivado**: se
//! faltar/corromper, reconstrói-se a partir dos segmentos. A consulta devolve ponteiros.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::codec;
use super::index::{Index, Query};
use super::record::{self, AuditRecord};
use crate::diag::{Level, events};
use crate::kernel::{Event, audit_dir};
use crate::ports::{Fs, FsError};
use crate::toon::{Section, emit};

/// Versão do esquema do manifesto.
pub const AUDIT_SCHEMA_VERSION: u32 = 1;

/// Eventos por segmento antes de selar.
pub const SEGMENT_EVENTS: usize = 256;

/// Erro da auditoria.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuditError {
    /// Falha de I/O.
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    /// Manifesto inválido.
    #[error("manifesto: {0}")]
    Manifest(String),
    /// Segmento inválido.
    #[error("segmento: {0}")]
    Parse(String),
}

/// Informação de um segmento selado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// Nome base (ex.: `seg-000001`).
    pub name: String,
    /// Primeiro `seq` incluído.
    pub from: u64,
    /// Último `seq` incluído.
    pub to: u64,
    /// Número de eventos.
    pub events: u64,
}

/// Manifesto da auditoria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Segmentos (ordem de selagem).
    pub segments: Vec<SegmentInfo>,
    /// Próximo número de segmento.
    pub next: u32,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema_version: AUDIT_SCHEMA_VERSION,
            segments: Vec::new(),
            next: 1,
        }
    }
}

/// Um resultado de consulta (ponteiro para o segmento).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Nome do segmento.
    pub seg: String,
    /// Linha dentro do segmento.
    pub ln: u32,
    /// `seq` do log.
    pub seq: u64,
    /// Tipo do evento.
    pub kind: String,
    /// Tool.
    pub tool: String,
    /// Caminho.
    pub path: String,
    /// Estado.
    pub status: String,
    /// Excerto.
    pub preview: String,
}

/// Armazenamento de auditoria por projeto.
pub struct AuditStore<'a> {
    fs: &'a dyn Fs,
    dir: PathBuf,
    manifest: Manifest,
    buffer: Vec<AuditRecord>,
}

impl<'a> AuditStore<'a> {
    /// Abre (ou cria) o armazenamento em `<root>/.katu/audit`.
    ///
    /// # Errors
    /// [`AuditError`] se o manifesto existente for inválido.
    pub fn open(fs: &'a dyn Fs, root: &Path) -> Result<Self, AuditError> {
        let dir = audit_dir(root);
        fs.create_dir_all(&dir)?;
        let manifest = codec::read_manifest(fs, &dir)?;
        Ok(Self {
            fs,
            dir,
            manifest,
            buffer: Vec::new(),
        })
    }

    /// Manifesto corrente.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Anexa um evento (sela automaticamente ao atingir [`SEGMENT_EVENTS`]).
    ///
    /// # Errors
    /// [`AuditError`] se a selagem falhar.
    pub fn append_event(&mut self, seq: u64, event: &Event) -> Result<(), AuditError> {
        self.buffer.push(AuditRecord::from_event(seq, event));
        if self.buffer.len() >= SEGMENT_EVENTS {
            self.flush()?;
        }
        Ok(())
    }

    /// Sela o segmento em buffer (no-op se vazio).
    ///
    /// # Errors
    /// [`AuditError`] se a escrita falhar.
    pub fn flush(&mut self) -> Result<(), AuditError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let _span = crate::span!(
            Level::Debug,
            events::AUDIT_SEAL,
            "events" => self.buffer.len(),
        );
        let index = Index::build(&self.buffer);
        let name = format!("seg-{:06}", self.manifest.next);
        let rec = emit(&[Section::Rows(record::table(&self.buffer))]);
        let idx = emit(&[Section::Rows(codec::index_table(&index))]);
        let rec_path = self.dir.join(format!("{name}.rec"));
        self.fs.write_atomic(&rec_path, rec.as_bytes())?;
        let idx_path = self.dir.join(format!("{name}.idx"));
        self.fs.write_atomic(&idx_path, idx.as_bytes())?;
        let from = self.buffer.first().map_or(0, |record| record.seq);
        let to = self.buffer.last().map_or(0, |record| record.seq);
        let events = u64::try_from(self.buffer.len()).unwrap_or(u64::MAX);
        self.manifest.segments.push(SegmentInfo {
            name,
            from,
            to,
            events,
        });
        self.manifest.next = self.manifest.next.saturating_add(1);
        codec::write_manifest(self.fs, &self.dir, &self.manifest)?;
        self.buffer.clear();
        Ok(())
    }

    /// Consulta os segmentos (mais recentes primeiro) e devolve até `limit` resultados.
    ///
    /// # Errors
    /// [`AuditError`] se um segmento for ilegível.
    pub fn search(&self, query: &Query, limit: usize) -> Result<Vec<Hit>, AuditError> {
        let _span = crate::span!(Level::Debug, events::AUDIT_QUERY);
        let mut hits = Vec::new();
        for info in self.manifest.segments.iter().rev() {
            let records = self.read_records(&info.name)?;
            let index = self
                .read_index(&info.name)?
                .unwrap_or_else(|| Index::build(&records));
            let mut lines: Vec<u32> = Vec::new();
            for group in &query.groups {
                lines.extend(index.matches(&records, group));
            }
            lines.sort_unstable();
            lines.dedup();
            for ln in lines.into_iter().rev() {
                if let Some(record) = records.get(usize::try_from(ln).unwrap_or(usize::MAX)) {
                    hits.push(hit(&info.name, ln, record));
                }
            }
        }
        hits.sort_by_key(|hit| Reverse(hit.seq));
        hits.truncate(limit);
        Ok(hits)
    }

    /// Lê as linhas de um segmento.
    fn read_records(&self, name: &str) -> Result<Vec<AuditRecord>, AuditError> {
        let text = codec::read_text(self.fs, &self.dir.join(format!("{name}.rec")))?;
        let rows = codec::parse_rows(&text, "a");
        rows.iter().map(|row| codec::parse_record(row)).collect()
    }

    /// Lê o índice derivado (ausente → `None`; o chamador reconstrói).
    fn read_index(&self, name: &str) -> Result<Option<Index>, AuditError> {
        let path = self.dir.join(format!("{name}.idx"));
        if !self.fs.exists(&path) {
            return Ok(None);
        }
        let text = codec::read_text(self.fs, &path)?;
        Ok(Some(codec::parse_index(&codec::parse_rows(&text, "t"))))
    }
}

fn hit(seg: &str, ln: u32, record: &AuditRecord) -> Hit {
    Hit {
        seg: seg.to_string(),
        ln,
        seq: record.seq,
        kind: record.kind.to_string(),
        tool: record.tool.clone(),
        path: record.path.clone(),
        status: record.status.clone(),
        preview: record.text.clone(),
    }
}

#[cfg(test)]
mod tests;
