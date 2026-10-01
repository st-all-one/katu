//! Armazenamento de auditoria (ADR 0009): manifesto, segmentos colunares e índice derivado.
//!
//! Segmentos selados são **imutáveis** (`seg-<NNNNNN>.rec` + `.idx`). O índice é **derivado**: se
//! faltar/corromper, reconstrói-se a partir dos segmentos. A consulta devolve ponteiros.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::bin;
use super::bloom::Bloom;
use super::codec;
use super::index::{Index, Query};
use super::record::{self, AuditRecord};
use crate::diag::{Level, events};
use crate::kernel::{Event, audit_dir};
use crate::ports::{Fs, FsError};
use crate::report::content_hash;
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
    /// Adulteração detetada (D2): a cadeia de hash está quebrada.
    #[error("adulteração no segmento {segment}: esperado {expected}, encontrado {found}")]
    Tamper {
        /// Segmento adulterado.
        segment: String,
        /// Hash esperado.
        expected: String,
        /// Hash encontrado.
        found: String,
    },
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
    /// Hash do conteúdo do segmento (FNV-1a 64-bit, 16 hex) — D2: deteção de adulteração.
    pub hash: String,
    /// Hash do segmento anterior (cadeia de hash; vazio no primeiro) — D2.
    pub prev_hash: String,
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
        let _span = crate::trace_fn!("audit::store::default");

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
        let _span = crate::trace_fn!("audit::store::open");

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
        let _span = crate::trace_fn!("audit::store::manifest");

        &self.manifest
    }

    /// Anexa um evento (sela automaticamente ao atingir [`SEGMENT_EVENTS`]).
    ///
    /// # Errors
    /// [`AuditError`] se a selagem falhar.
    pub fn append_event(&mut self, seq: u64, event: &Event) -> Result<(), AuditError> {
        let _span = crate::trace_fn!("audit::store::append_event");

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
        let _span = crate::trace_fn!("audit::store::flush");

        if self.buffer.is_empty() {
            return Ok(());
        }
        let _span = crate::fn_span!(
            Level::Debug,
            events::AUDIT_SEAL,
            "audit::store::flush",
            "events" => self.buffer.len(),
        );
        let index = Index::build(&self.buffer);
        let bloom = Bloom::from_terms(index.postings().keys().map(String::as_str));
        let name = format!("seg-{:06}", self.manifest.next);
        let rec = emit(&[Section::Rows(record::table(&self.buffer))]);
        let idx = bin::encode(&index, &bloom);
        let rec_path = self.dir.join(format!("{name}.rec"));
        self.fs.write_atomic(&rec_path, rec.as_bytes())?;
        let idx_path = self.dir.join(format!("{name}.idx"));
        self.fs.write_atomic(&idx_path, &idx)?;
        let from = self.buffer.first().map_or(0, |record| record.seq);
        let to = self.buffer.last().map_or(0, |record| record.seq);
        let events = u64::try_from(self.buffer.len()).unwrap_or(u64::MAX);
        // D2: cadeia de hash — o hash do segmento é o hash do seu conteúdo; o `prev_hash` é o
        // hash do segmento anterior (vazio no primeiro). Qualquer adulteração quebra a cadeia.
        let prev_hash = self
            .manifest
            .segments
            .last()
            .map_or(String::new(), |seg| seg.hash.clone());
        let hash = content_hash(rec.as_bytes());
        self.manifest.segments.push(SegmentInfo {
            name,
            from,
            to,
            events,
            hash,
            prev_hash,
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
        let _span = crate::fn_span!(Level::Debug, events::AUDIT_QUERY, "audit::store::search");
        let mut hits = Vec::new();
        for info in self.manifest.segments.iter().rev() {
            let (index, bloom) = if let Some(stored) = self.read_index(&info.name)? {
                stored
            } else {
                let index = Index::build(&self.read_records(&info.name)?);
                let bloom = Bloom::from_terms(index.postings().keys().map(String::as_str));
                (index, bloom)
            };
            if !might_match(&bloom, query) {
                continue;
            }
            let records = self.read_records(&info.name)?;
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
        let _span = crate::trace_fn!("audit::store::read_records");

        let text = codec::read_text(self.fs, &self.dir.join(format!("{name}.rec")))?;
        let rows = codec::parse_rows(&text, "a");
        rows.iter().map(|row| codec::parse_record(row)).collect()
    }

    /// Lê o índice derivado (ausente → `None`; o chamador reconstrói).
    fn read_index(&self, name: &str) -> Result<Option<StoredIndex>, AuditError> {
        let _span = crate::trace_fn!("audit::store::read_index");

        let path = self.dir.join(format!("{name}.idx"));
        if !self.fs.exists(&path) {
            return Ok(None);
        }
        Ok(bin::decode(&self.fs.read(&path)?))
    }

    /// Verifica a cadeia de hash dos segmentos (D2: deteção de adulteração).
    ///
    /// Para cada segmento, lê o `.rec`, computa o hash e compara com o `hash` do manifesto;
    /// verifica também que o `prev_hash` liga ao segmento anterior. Qualquer adulteração
    /// (conteúdo modificado, segmento removido, ordem trocada) é detetada.
    ///
    /// # Errors
    /// [`AuditError::Tamper`] se a cadeia estiver quebrada.
    pub fn verify(&self) -> Result<(), AuditError> {
        let _span = crate::fn_span!(Level::Debug, events::AUDIT_SEAL, "audit::store::verify");
        let mut prev_hash = String::new();
        for info in &self.manifest.segments {
            let path = self.dir.join(format!("{}.rec", info.name));
            let bytes = self.fs.read(&path)?;
            let computed = content_hash(&bytes);
            if computed != info.hash {
                return Err(AuditError::Tamper {
                    segment: info.name.clone(),
                    expected: info.hash.clone(),
                    found: computed,
                });
            }
            if info.prev_hash != prev_hash {
                return Err(AuditError::Tamper {
                    segment: info.name.clone(),
                    expected: prev_hash.clone(),
                    found: info.prev_hash.clone(),
                });
            }
            prev_hash.clone_from(&info.hash);
        }
        Ok(())
    }
}

/// Índice + Bloom lidos de um segmento.
type StoredIndex = (Index, Bloom);

/// `true` se algum grupo da consulta pode existir no segmento (o Bloom só prova ausência).
fn might_match(bloom: &Bloom, query: &Query) -> bool {
    let _span = crate::trace_fn!("audit::store::might_match");

    query.groups.is_empty()
        || query.groups.iter().any(|group| {
            group.terms.iter().all(|term| bloom.might_contain(term))
                && group
                    .phrases
                    .iter()
                    .all(|phrase| phrase.first().is_none_or(|word| bloom.might_contain(word)))
        })
}

fn hit(seg: &str, ln: u32, record: &AuditRecord) -> Hit {
    let _span = crate::trace_fn!("audit::store::hit");

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
