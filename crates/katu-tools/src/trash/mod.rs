//! Tool `trash` (E06-T09): move para `.katu/trash` (recuperável), **nunca** apaga.
//!
//! A lixeira é **por projeto** (`<root>/.katu/trash`), fora do escopo de escrita normal e nunca
//! servida ao modelo por omissão. `trash` **move** (não copia+apaga) e regista um índice com o
//! original + timestamp; devolve `undo_token`. `restore` é **sempre** permitido (não passa pela
//! política) e nada é apagado automaticamente (sem TTL/auto-purge).

mod index;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Clock, Fs};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

use index::TrashRecord;

/// Número máximo de colisões antes de desistir.
const MAX_COLLISIONS: u32 = 1000;

/// Executor de envio para a lixeira.
pub struct TrashTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Porta de relógio (índice + desambiguação).
    pub clock: &'a dyn Clock,
    /// Raiz do projeto.
    pub root: PathBuf,
}

impl Tool for TrashTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Trash
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_TRASH);
        let ToolArgs::Trash { path } = &use_.args else {
            return unavailable("trash");
        };
        let original = Path::new(path.as_str());
        let Ok(bytes) = self.fs.read(original) else {
            return unavailable("missing");
        };
        let now = self.clock.now().as_millis();
        let Some(stored) = self.unique(original, now) else {
            return unavailable("collision");
        };
        let Some(parent) = stored.parent() else {
            return unavailable("path");
        };
        if self.fs.create_dir_all(parent).is_err() {
            return unavailable("mkdir");
        }
        if self.fs.rename(original, &stored).is_err() {
            return unavailable("move");
        }
        let record = TrashRecord {
            at_millis: now,
            stored: stored.display().to_string(),
            original: path.as_str().to_string(),
        };
        if index::append(self.fs, &self.root, &record).is_err() {
            return unavailable("index");
        }
        ToolOutput::report(build(path.as_str(), &stored, &bytes, now))
    }
}

impl TrashTool<'_> {
    /// Diretório da lixeira (por projeto).
    fn trash_dir(&self) -> PathBuf {
        self.root.join(".katu").join("trash")
    }

    /// Caminho guardado livre (preserva o relativo; desambigua em colisão).
    fn unique(&self, original: &Path, now: u64) -> Option<PathBuf> {
        let base = self.stored_base(original);
        if !self.fs.exists(&base) {
            return Some(base);
        }
        let name = base
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        for index in 0..MAX_COLLISIONS {
            let candidate = base.with_file_name(format!("{name}.{now}.{index}"));
            if !self.fs.exists(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// Caminho base na lixeira (com o caminho relativo preservado).
    fn stored_base(&self, original: &Path) -> PathBuf {
        let relative = original
            .strip_prefix(&self.root)
            .ok()
            .filter(|rel| !rel.as_os_str().is_empty());
        match relative {
            Some(rel) => self.trash_dir().join(rel),
            None => self
                .trash_dir()
                .join(original.file_name().unwrap_or_else(|| OsStr::new("file"))),
        }
    }
}

/// Item da lixeira na visão do utilizador (subset do índice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    /// Caminho guardado (token de restauro).
    pub stored: String,
    /// Caminho original.
    pub original: String,
    /// Instante (ms) em que foi enviado para a lixeira.
    pub at_millis: u64,
}

/// Lista a lixeira, do mais recente para o mais antigo (vazia sem índice).
///
/// Itens já removidos (lixeira esvaziada) deixam de aparecer: filtra pela **existência** do
/// ficheiro guardado, preservando o índice append-only como rasto de auditoria.
#[must_use]
pub fn list(fs: &dyn Fs, root: &Path) -> Vec<TrashItem> {
    let mut items: Vec<TrashItem> = index::read(fs, root)
        .into_iter()
        .filter(|record| fs.exists(Path::new(&record.stored)))
        .map(|record| TrashItem {
            stored: record.stored,
            original: record.original,
            at_millis: record.at_millis,
        })
        .collect();
    items.reverse();
    items
}

/// Restaura um item guardado. **Sempre permitido** (não passa pela política).
///
/// # Errors
/// Devolve [`TrashError`] se o token for desconhecido, o original estiver ocupado ou houver I/O.
pub fn restore(fs: &dyn Fs, root: &Path, token: &str) -> Result<PathBuf, TrashError> {
    let records = index::read(fs, root);
    let record = records
        .iter()
        .rev()
        .find(|record| record.stored == token)
        .ok_or(TrashError::Unknown)?;
    let original = PathBuf::from(&record.original);
    if fs.exists(&original) {
        return Err(TrashError::Occupied);
    }
    if let Some(parent) = original.parent() {
        fs.create_dir_all(parent).map_err(|_| TrashError::Io)?;
    }
    fs.rename(Path::new(token), &original)
        .map_err(|_| TrashError::Io)?;
    Ok(original)
}

/// Esvazia a lixeira: remove **permanentemente** os ficheiros guardados e devolve quantos removeu.
///
/// Ação **destrutiva** (exige challenge-and-response na UI, E10-T07); o índice append-only é
/// preservado como rasto de auditoria e [`list`] deixa de mostrar os itens removidos.
///
/// # Errors
/// [`TrashError::Io`] se uma remoção falhar.
pub fn empty(fs: &dyn Fs, root: &Path) -> Result<usize, TrashError> {
    let records = index::read(fs, root);
    let mut removed = 0usize;
    for record in &records {
        let stored = Path::new(&record.stored);
        if !fs.exists(stored) {
            continue;
        }
        fs.remove(stored).map_err(|_| TrashError::Io)?;
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}

/// Erro de restauro.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrashError {
    /// Token desconhecido (não consta do índice).
    Unknown,
    /// O caminho original já está ocupado.
    Occupied,
    /// Erro de I/O.
    Io,
}

impl std::fmt::Display for TrashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("token de lixeira desconhecido"),
            Self::Occupied => f.write_str("o caminho original já está ocupado"),
            Self::Io => f.write_str("erro de I/O na lixeira"),
        }
    }
}

impl std::error::Error for TrashError {}

fn build(original: &str, stored: &Path, bytes: &[u8], now: u64) -> ToolReport {
    let id = content_id("f", original.as_bytes());
    let hash = content_hash(bytes);
    let token = stored.display().to_string();
    let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    let at = i64::try_from(now).unwrap_or(i64::MAX);
    let data = Value::map(vec![
        ("original".to_string(), Value::str(original)),
        ("stored".to_string(), Value::str(token.clone())),
        ("undo_token".to_string(), Value::str(token.clone())),
        (
            "refs".to_string(),
            Value::list(vec![Value::str(id.clone())]),
        ),
        ("bytes".to_string(), Value::int(size)),
        ("at".to_string(), Value::int(at)),
    ]);
    let next = vec![format!("restore {token}")];
    ToolReport::new("trash.move", data)
        .with_id(id)
        .with_hash(hash)
        .with_next(next)
}

fn unavailable(control: &'static str) -> ToolOutput {
    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests;
