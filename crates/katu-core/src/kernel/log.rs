//! Log de sessão **append-only** em JSONL (E04-T02), sobre a porta [`Fs`].
//!
//! Uma geração por esquema: `session.v{N}.jsonl`. Cada linha é um [`LogRecord`] com um `seq`
//! contíguo a partir de 1; um salto de `seq` ou uma linha ilegível é **corrupção** (fail-closed).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::event::Event;
use crate::diag::{Level, events};
use crate::ports::{Fs, FsError};

/// Versão do esquema do log; faz parte do nome do ficheiro.
pub const LOG_SCHEMA_VERSION: u32 = 1;

/// Registo append-only: número de sequência + evento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    /// Número de sequência (começa em 1, contíguo).
    pub seq: u64,
    /// Evento.
    pub event: Event,
}

/// Natureza de erro do log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LogErrorKind {
    /// Falha de I/O.
    Io,
    /// Linha/serialização inválida.
    Corrupt,
    /// `seq` não contíguo (truncagem/adulteração).
    SequenceGap,
}

/// Erro do log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct LogError {
    /// Natureza.
    pub kind: LogErrorKind,
    /// Mensagem legível.
    pub message: String,
}

impl LogError {
    /// Constrói um erro do log.
    fn new(kind: LogErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Caminho canônico da geração atual do log.
#[must_use]
pub fn session_path(dir: &Path) -> PathBuf {
    dir.join(format!("session.v{LOG_SCHEMA_VERSION}.jsonl"))
}

/// Escritor append-only de uma sessão.
pub struct Log<'a> {
    fs: &'a dyn Fs,
    path: PathBuf,
    seq: u64,
    offset: u64,
}

impl<'a> Log<'a> {
    /// Abre (ou cria) o log no diretório dado, retomando a sequência existente.
    ///
    /// # Errors
    /// [`LogError`] se o log existente estiver corrompido.
    pub fn open(fs: &'a dyn Fs, dir: &Path) -> Result<Self, LogError> {
        let path = session_path(dir);
        let (records, len) = read_records_with_len(fs, &path)?;
        let seq = records.last().map_or(0, |record| record.seq);
        Ok(Self {
            fs,
            path,
            seq,
            offset: u64::try_from(len).unwrap_or(u64::MAX),
        })
    }

    /// Retoma o log a partir dos valores já conhecidos (sem reler o ficheiro — ADR 0008).
    pub(crate) fn resume(fs: &'a dyn Fs, dir: &Path, seq: u64, offset: u64) -> Self {
        Self {
            fs,
            path: session_path(dir),
            seq,
            offset,
        }
    }

    /// Caminho do ficheiro de log.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Último `seq` gravado (`0` se vazio).
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Offset (bytes) do início da próxima linha.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Anexa um evento e devolve o `seq` atribuído.
    ///
    /// # Errors
    /// [`LogError`] se a serialização ou o I/O falharem.
    pub fn append(&mut self, event: &Event) -> Result<u64, LogError> {
        let seq = self.seq.saturating_add(1);
        let _span = crate::span!(Level::Trace, events::LOG_APPEND, "seq" => seq);
        let record = LogRecord {
            seq,
            event: event.clone(),
        };
        let mut line = serde_json::to_vec(&record)
            .map_err(|err| LogError::new(LogErrorKind::Corrupt, err.to_string()))?;
        line.push(b'\n');
        self.fs.append(&self.path, &line).map_err(from_io)?;
        self.seq = seq;
        self.offset = self
            .offset
            .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        Ok(seq)
    }
}

/// Lê e valida todos os registos de um log (vazio se o ficheiro não existir).
///
/// # Errors
/// [`LogError`] se o ficheiro não for UTF-8, tiver uma linha ilegível ou um salto de `seq`.
pub fn read_records(fs: &dyn Fs, path: &Path) -> Result<Vec<LogRecord>, LogError> {
    read_records_with_len(fs, path).map(|(records, _)| records)
}

/// Lê todos os registos e devolve também o tamanho (bytes) do ficheiro.
pub(super) fn read_records_with_len(
    fs: &dyn Fs,
    path: &Path,
) -> Result<(Vec<LogRecord>, usize), LogError> {
    let _span = crate::span!(Level::Trace, events::LOG_REPLAY);
    if !fs.exists(path) {
        return Ok((Vec::new(), 0));
    }
    let bytes = fs.read(path).map_err(from_io)?;
    let len = bytes.len();
    Ok((parse_records(&bytes, 1)?, len))
}

/// Lê só a cauda a partir de `offset` (retomada incremental, ADR 0008) e devolve o tamanho total.
///
/// # Errors
/// [`LogError`] se o ficheiro não for UTF-8 ou a cauda tiver uma linha ilegível/salto de `seq`.
pub(super) fn read_records_from(
    fs: &dyn Fs,
    path: &Path,
    offset: u64,
    first_seq: u64,
) -> Result<(Vec<LogRecord>, usize), LogError> {
    let _span = crate::span!(Level::Trace, events::LOG_REPLAY);
    let tail = fs.read_from(path, offset).map_err(from_io)?;
    let len = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .saturating_add(tail.len());
    Ok((parse_records(&tail, first_seq)?, len))
}

/// Valida e desserializa as linhas de um log, exigindo `seq` contíguo desde `first_seq`.
fn parse_records(bytes: &[u8], first_seq: u64) -> Result<Vec<LogRecord>, LogError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|err| LogError::new(LogErrorKind::Corrupt, format!("log não é UTF-8: {err}")))?;
    let mut records = Vec::new();
    let mut expected = first_seq;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: LogRecord = serde_json::from_str(line).map_err(|err| {
            LogError::new(LogErrorKind::Corrupt, format!("linha inválida: {err}"))
        })?;
        if record.seq != expected {
            return Err(LogError::new(
                LogErrorKind::SequenceGap,
                format!("seq esperado {expected}, veio {}", record.seq),
            ));
        }
        expected = expected.saturating_add(1);
        records.push(record);
    }
    Ok(records)
}

/// Converte um erro da porta `Fs` num erro do log.
fn from_io(err: FsError) -> LogError {
    match err {
        FsError::NotFound => LogError::new(LogErrorKind::Io, "caminho não encontrado"),
        FsError::Stale => LogError::new(LogErrorKind::Io, "conteúdo mudou desde a leitura"),
        FsError::Io(message) => LogError::new(LogErrorKind::Io, message),
    }
}

#[cfg(test)]
mod tests;
