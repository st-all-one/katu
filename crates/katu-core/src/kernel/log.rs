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
}

impl<'a> Log<'a> {
    /// Abre (ou cria) o log no diretório dado, retomando a sequência existente.
    ///
    /// # Errors
    /// [`LogError`] se o log existente estiver corrompido.
    pub fn open(fs: &'a dyn Fs, dir: &Path) -> Result<Self, LogError> {
        let path = session_path(dir);
        let records = read_records(fs, &path)?;
        let seq = records.last().map_or(0, |record| record.seq);
        Ok(Self { fs, path, seq })
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
        Ok(seq)
    }
}

/// Lê e valida todos os registos de um log (vazio se o ficheiro não existir).
///
/// # Errors
/// [`LogError`] se o ficheiro não for UTF-8, tiver uma linha ilegível ou um salto de `seq`.
pub fn read_records(fs: &dyn Fs, path: &Path) -> Result<Vec<LogRecord>, LogError> {
    let _span = crate::span!(Level::Trace, events::LOG_REPLAY);
    if !fs.exists(path) {
        return Ok(Vec::new());
    }
    let bytes = fs.read(path).map_err(from_io)?;
    let text = String::from_utf8(bytes)
        .map_err(|err| LogError::new(LogErrorKind::Corrupt, format!("log não é UTF-8: {err}")))?;
    let mut records = Vec::new();
    let mut expected: u64 = 1;
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
mod tests {
    use super::{Log, LogErrorKind, read_records, session_path};
    use crate::error::ToolOutcome;
    use crate::kernel::{CallId, Event, derive_messages, state_of};
    use crate::ports::{Fs, MemFs};
    use katu_policy::{Phase, ResolvedPath, ToolArgs, ToolName, ToolUse};
    use std::path::Path;

    #[test]
    fn append_then_read_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let mut log = Log::open(&fs, Path::new("/sessions"))?;
        assert_eq!(log.seq(), 0);
        assert_eq!(log.append(&Event::TurnStart { turn: 1 })?, 1);
        assert_eq!(log.append(&Event::TurnEnd { turn: 1 })?, 2);

        let records = read_records(&fs, log.path())?;
        assert_eq!(records.len(), 2);
        assert_eq!(records.first().map(|record| record.seq), Some(1));
        assert_eq!(records.get(1).map(|record| record.seq), Some(2));
        Ok(())
    }

    #[test]
    fn reopen_resumes_sequence() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let dir = Path::new("/sessions");
        {
            let mut log = Log::open(&fs, dir)?;
            log.append(&Event::TurnStart { turn: 1 })?;
        }
        let mut reopened = Log::open(&fs, dir)?;
        assert_eq!(reopened.seq(), 1);
        assert_eq!(reopened.append(&Event::TurnEnd { turn: 1 })?, 2);
        Ok(())
    }

    #[test]
    fn sequence_gap_is_detected() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let path = session_path(Path::new("/sessions"));
        let line = b"{\"seq\":1,\"event\":{\"type\":\"turn_start\",\"turn\":1}}\n\
                     {\"seq\":3,\"event\":{\"type\":\"turn_end\",\"turn\":1}}\n";
        fs.write_atomic(&path, line)?;
        let err = read_records(&fs, &path).err();
        assert!(err.is_some_and(|error| error.kind == LogErrorKind::SequenceGap));
        Ok(())
    }

    #[test]
    fn corrupt_line_is_detected() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let path = session_path(Path::new("/sessions"));
        fs.write_atomic(&path, b"{nao e json}\n")?;
        let err = read_records(&fs, &path).err();
        assert!(err.is_some_and(|error| error.kind == LogErrorKind::Corrupt));
        Ok(())
    }

    #[test]
    fn replay_from_log_is_byte_stable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let dir = Path::new("/sessions");
        let call = CallId::new("c1");
        let events = vec![
            Event::TurnStart { turn: 7 },
            Event::UserMessage {
                text: "escreve".into(),
            },
            Event::ToolCall {
                call: call.clone(),
                tool: tool()?,
            },
            Event::ToolResult {
                call,
                outcome: ToolOutcome::Ok,
            },
            Event::Waiver {
                transition: Phase::KnowledgeConsulted,
                reason: "teste do log".into(),
            },
            Event::PhaseTransition {
                to: Phase::KnowledgeConsulted,
                outcome: None,
            },
            Event::TurnEnd { turn: 7 },
        ];
        {
            let mut log = Log::open(&fs, dir)?;
            for event in &events {
                log.append(event)?;
            }
        }
        let recovered: Vec<Event> = read_records(&fs, &session_path(dir))?
            .into_iter()
            .map(|record| record.event)
            .collect();
        assert_eq!(recovered, events, "o log deve reidratar byte-a-byte");
        assert_eq!(state_of(&recovered)?, state_of(&events)?);
        assert_eq!(derive_messages(&recovered), derive_messages(&events));
        Ok(())
    }

    fn tool() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
        Ok(ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: path.clone(),
                bytes: 1,
            },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }
}
