//! Log de sessão **append-only** em JSONL (E04-T02), sobre a porta [`Fs`].
//!
//! Uma geração por esquema: `session.v{N}.jsonl`. Cada linha é um [`LogRecord`] com um `seq`
//! contíguo a partir de 1; um salto de `seq` ou uma linha ilegível é **corrupção** (fail-closed).
//!
//! **Durabilidade (ADR 0024, P-01).** [`Durability::Event`] (default) sincroniza em cada `append`;
//! [`Durability::Turn`] escreve sem sincronizar e faz a barreira em [`Log::flush`], chamada na
//! fronteira do turno — é o *group commit*, que troca uma janela de perda de um turno por um
//! `fsync` em vez de um por evento. A **cauda rasgada** de um crash é recuperada (última linha
//! incompleta descartada, com aviso); a corrupção a meio continua a ser erro.

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

/// Política de durabilidade do log (ADR 0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Durability {
    /// Uma barreira por evento: um evento aceite está no disco (default histórico).
    #[default]
    Event,
    /// Uma barreira por turno: os turnos anteriores estão no disco, o corrente pode perder-se.
    Turn,
}

impl Durability {
    /// Nome estável (config/diagnóstico).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Turn => "turn",
        }
    }

    /// Interpreta o valor da config; `None` para um valor desconhecido (fail-closed no chamador).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "event" => Some(Self::Event),
            "turn" => Some(Self::Turn),
            _ => None,
        }
    }
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
        let _span = crate::trace_fn!("kernel::log::new");

        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Caminho canônico da geração atual do log.
#[must_use]
pub fn session_path(dir: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::log::session_path");

    dir.join(format!("session.v{LOG_SCHEMA_VERSION}.jsonl"))
}

/// Escritor append-only de uma sessão.
pub struct Log<'a> {
    fs: &'a dyn Fs,
    path: PathBuf,
    seq: u64,
    offset: u64,
    durability: Durability,
    /// Há escritas por sincronizar (só no modo [`Durability::Turn`]).
    dirty: bool,
}

impl<'a> Log<'a> {
    /// Abre (ou cria) o log no diretório dado, retomando a sequência existente.
    ///
    /// # Errors
    /// [`LogError`] se o log existente estiver corrompido.
    pub fn open(fs: &'a dyn Fs, dir: &Path) -> Result<Self, LogError> {
        let _span = crate::fn_span!(Level::Trace, events::LOG_REPLAY, "kernel::log::open");
        let path = session_path(dir);
        let (records, len) = read_records_with_len(fs, &path)?;
        let seq = records.last().map_or(0, |record| record.seq);
        Ok(Self {
            fs,
            path,
            seq,
            offset: u64::try_from(len).unwrap_or(u64::MAX),
            durability: Durability::Event,
            dirty: false,
        })
    }

    /// Define a política de durabilidade (ADR 0024); default [`Durability::Event`].
    pub fn set_durability(&mut self, durability: Durability) {
        let _span = crate::trace_fn!("kernel::log::set_durability");

        self.durability = durability;
    }

    /// Política em vigor.
    #[must_use]
    pub const fn durability(&self) -> Durability {
        self.durability
    }

    /// Retoma o log a partir dos valores já conhecidos (sem reler o ficheiro — ADR 0008).
    pub(crate) fn resume(fs: &'a dyn Fs, dir: &Path, seq: u64, offset: u64) -> Self {
        let _span = crate::trace_fn!("kernel::log::resume");

        Self {
            fs,
            path: session_path(dir),
            seq,
            offset,
            durability: Durability::Event,
            dirty: false,
        }
    }

    /// Caminho do ficheiro de log.
    #[must_use]
    pub fn path(&self) -> &Path {
        let _span = crate::trace_fn!("kernel::log::path");

        &self.path
    }

    /// Último `seq` gravado (`0` se vazio).
    #[must_use]
    pub fn seq(&self) -> u64 {
        let _span = crate::trace_fn!("kernel::log::seq");

        self.seq
    }

    /// Offset (bytes) do início da próxima linha.
    #[must_use]
    pub fn offset(&self) -> u64 {
        let _span = crate::trace_fn!("kernel::log::offset");

        self.offset
    }

    /// Anexa um evento e devolve o `seq` atribuído.
    ///
    /// # Errors
    /// [`LogError`] se a serialização ou o I/O falharem.
    pub fn append(&mut self, event: &Event) -> Result<u64, LogError> {
        let _span = crate::trace_fn!("kernel::log::append");

        let seq = self.seq.saturating_add(1);
        let _span =
            crate::fn_span!(Level::Trace, events::LOG_APPEND, "kernel::log::append", "seq" => seq);
        let record = LogRecord {
            seq,
            event: event.clone(),
        };
        let mut line = serde_json::to_vec(&record)
            .map_err(|err| LogError::new(LogErrorKind::Corrupt, err.to_string()))?;
        line.push(b'\n');
        // Modo `Turn`: escreve sem barreira e marca a pendência; `flush` fecha-a no fim do turno.
        match self.durability {
            Durability::Event => self.fs.append(&self.path, &line).map_err(from_io)?,
            Durability::Turn => {
                self.fs
                    .append_unsynced(&self.path, &line)
                    .map_err(from_io)?;
                self.dirty = true;
            }
        }
        self.seq = seq;
        self.offset = self
            .offset
            .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        Ok(seq)
    }
}

impl Log<'_> {
    /// Fecha a barreira pendente do *group commit* (no-op no modo [`Durability::Event`]).
    ///
    /// É chamada na fronteira do turno: depois disto, todos os eventos anexados estão no disco.
    ///
    /// # Errors
    /// [`LogError`] se a barreira falhar (o log **não** é dado como durável).
    pub fn flush(&mut self) -> Result<(), LogError> {
        let _span = crate::fn_span!(Level::Trace, events::LOG_APPEND, "kernel::log::flush");
        if !self.dirty {
            return Ok(());
        }
        self.fs.sync(&self.path).map_err(from_io)?;
        self.dirty = false;
        Ok(())
    }

    /// `true` se há escritas por sincronizar.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// Lê e valida todos os registos de um log (vazio se o ficheiro não existir).
///
/// # Errors
/// [`LogError`] se o ficheiro não for UTF-8, tiver uma linha ilegível ou um salto de `seq`.
pub fn read_records(fs: &dyn Fs, path: &Path) -> Result<Vec<LogRecord>, LogError> {
    let _span = crate::trace_fn!("kernel::log::read_records");

    read_records_with_len(fs, path).map(|(records, _)| records)
}

/// Lê todos os registos e devolve também o tamanho (bytes) do ficheiro.
pub(super) fn read_records_with_len(
    fs: &dyn Fs,
    path: &Path,
) -> Result<(Vec<LogRecord>, usize), LogError> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::LOG_REPLAY,
        "kernel::log::read_records_with_len"
    );
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
    let _span = crate::fn_span!(
        Level::Trace,
        events::LOG_REPLAY,
        "kernel::log::read_records_from"
    );
    let tail = fs.read_from(path, offset).map_err(from_io)?;
    let len = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .saturating_add(tail.len());
    Ok((parse_records(&tail, first_seq)?, len))
}

/// Valida e desserializa as linhas de um log, exigindo `seq` contíguo desde `first_seq`.
fn parse_records(bytes: &[u8], first_seq: u64) -> Result<Vec<LogRecord>, LogError> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::LOG_REPLAY,
        "kernel::log::parse_records"
    );
    // Um crash pode deixar a **última** linha incompleta (sem `\n`): descarta-se e recupera-se
    // (ADR 0024). A marca do registo rasgado é **não terminar em `\n`**: um ficheiro terminado é
    // íntegro, pelo que uma linha inválida aí é corrupção de verdade (fail-closed). Sem a
    // recuperação, um crash tornaria a sessão irrecuperável — e a política por turno trocaria
    // `fsync` por perda de sessão.
    let torn_tail = !bytes.ends_with(b"\n");
    let (text, torn) = match std::str::from_utf8(bytes) {
        Ok(text) => (text, None),
        Err(err) => {
            let valid = err.valid_up_to();
            let tail = bytes.get(valid..).unwrap_or(&[]);
            let recoverable =
                bytes.get(valid.saturating_sub(1)) != Some(&b'\n') && !tail.contains(&b'\n');
            if !recoverable {
                return Err(LogError::new(
                    LogErrorKind::Corrupt,
                    format!("log não é UTF-8: {err}"),
                ));
            }
            let text = std::str::from_utf8(bytes.get(..valid).unwrap_or(&[])).map_err(|err| {
                LogError::new(LogErrorKind::Corrupt, format!("log não é UTF-8: {err}"))
            })?;
            (text, Some(valid))
        }
    };
    let mut records = Vec::new();
    let mut expected = first_seq;
    let lines: Vec<&str> = text.lines().collect();
    let last = lines.len().saturating_sub(1);
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: LogRecord = match serde_json::from_str(line) {
            Ok(record) => record,
            // Linha rasgada: só a **última** é recuperável.
            Err(_) if index == last && torn_tail => {
                crate::event!(Level::Warn, events::LOG_REPLAY, "recovered" => true, "reason" => "cauda rasgada");
                break;
            }
            Err(err) => {
                return Err(LogError::new(
                    LogErrorKind::Corrupt,
                    format!("linha inválida: {err}"),
                ));
            }
        };
        if record.seq != expected {
            return Err(LogError::new(
                LogErrorKind::SequenceGap,
                format!("seq esperado {expected}, veio {}", record.seq),
            ));
        }
        expected = expected.saturating_add(1);
        records.push(record);
    }
    if torn.is_some() {
        crate::event!(Level::Warn, events::LOG_REPLAY, "recovered" => true, "reason" => "cauda não-UTF-8");
    }
    Ok(records)
}

/// Converte um erro da porta `Fs` num erro do log.
fn from_io(err: FsError) -> LogError {
    let _span = crate::trace_fn!("kernel::log::from_io");

    match err {
        FsError::NotFound => LogError::new(LogErrorKind::Io, "caminho não encontrado"),
        FsError::Stale => LogError::new(LogErrorKind::Io, "conteúdo mudou desde a leitura"),
        FsError::Io(message) => LogError::new(LogErrorKind::Io, message),
    }
}

#[cfg(test)]
mod tests;
