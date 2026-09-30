//! Identidade e vinculação de sessões (ADR 0008): `SessionId`, layout `.katu/`, índice temporal.
//!
//! O ID é derivado **na borda** de `(root canónico, created_ms)` — sem RNG no kernel. O índice
//! (`index.jsonl`) é append-only e a ordenação canónica é `(created_ms, id)`. A pasta de auditoria
//! fica **fora** do git local: garante-se, idempotentemente, a linha em `.git/info/exclude`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ports::{Fs, FsError};
use crate::report::fingerprint;

/// Versão do esquema de `meta.json`/`index.jsonl`.
pub(super) const SESSION_SCHEMA_VERSION: u32 = 1;

/// Linha adicionada a `.git/info/exclude` (audit só local, ADR 0009).
pub(super) const AUDIT_EXCLUDE_LINE: &str = ".katu/audit/";

/// Identificador estável de sessão (`s_<16hex>`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// Deriva o ID de `(root, created_ms)` de forma determinística.
    #[must_use]
    pub fn new(root: &Path, created_ms: u64) -> Self {
        let _span = crate::trace_fn!("kernel::session::identity::new");

        let seed = format!("{}\n{created_ms}", root.display());
        Self(format!("s_{:016x}", fingerprint(seed.as_bytes())))
    }

    /// Texto do identificador.
    #[must_use]
    pub fn as_str(&self) -> &str {
        let _span = crate::trace_fn!("kernel::session::identity::as_str");

        &self.0
    }

    /// Valida e reconstrói a partir do texto (`s_` + 16 hex).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let _span = crate::trace_fn!("kernel::session::identity::parse");

        let hex = text.strip_prefix("s_")?;
        if hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Self(text.to_string()))
        } else {
            None
        }
    }
}

/// Metadados estáveis de uma sessão (linha de `index.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Identificador.
    pub id: SessionId,
    /// Raiz do projeto (path exato de retomada).
    pub root: String,
    /// Instante de criação (ms desde a época).
    pub created_ms: u64,
    /// Objetivo declarado (pode ser vazio).
    pub goal: String,
}

/// Diretório `.katu` do projeto.
#[must_use]
pub fn katu_dir(root: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::katu_dir");

    root.join(".katu")
}

/// Diretório de sessões.
#[must_use]
pub(super) fn sessions_dir(root: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::sessions_dir");

    katu_dir(root).join("sessions")
}

/// Diretório de uma sessão.
#[must_use]
pub(super) fn session_dir(root: &Path, id: &SessionId) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::session_dir");

    sessions_dir(root).join(id.as_str())
}

/// Caminho do `meta.json` de uma sessão.
#[must_use]
pub(super) fn meta_path(dir: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::meta_path");

    dir.join("meta.json")
}

/// Caminho do índice temporal.
#[must_use]
pub(super) fn index_path(root: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::index_path");

    sessions_dir(root).join("index.jsonl")
}

/// Diretório de auditoria (`<root>/.katu/audit`).
#[must_use]
pub fn audit_dir(root: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::audit_dir");

    katu_dir(root).join("audit")
}

/// Descobre a raiz do projeto subindo até `.katu` ou `.git`; devolve `start` se não encontrar.
#[must_use]
pub fn discover_root(fs: &dyn Fs, start: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::identity::discover_root");

    let mut current = start.to_path_buf();
    loop {
        if fs.is_dir(&katu_dir(&current)) || fs.is_dir(&current.join(".git")) {
            return current;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => return start.to_path_buf(),
        }
    }
}

/// Cria o layout `.katu/` e a sessão, devolvendo os metadados. Idempotente em `.katu/`.
///
/// # Errors
/// [`FsError`] se a escrita do layout falhar.
pub(super) fn create(
    fs: &dyn Fs,
    root: &Path,
    created_ms: u64,
    goal: &str,
) -> Result<SessionMeta, FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::create");

    fs.create_dir_all(&sessions_dir(root))?;
    fs.create_dir_all(&audit_dir(root))?;
    let id = SessionId::new(root, created_ms);
    let dir = session_dir(root, &id);
    fs.create_dir_all(&dir)?;
    let meta = SessionMeta {
        schema_version: SESSION_SCHEMA_VERSION,
        id,
        root: root.display().to_string(),
        created_ms,
        goal: goal.to_string(),
    };
    let bytes = to_bytes(&meta)?;
    fs.write_atomic(&meta_path(&dir), &bytes)?;
    append_index(fs, root, &meta)?;
    ensure_audit_excluded(fs, root)?;
    Ok(meta)
}

/// Lê o `meta.json` de um diretório de sessão, se existir e for válido.
#[must_use]
pub(super) fn load_meta(fs: &dyn Fs, dir: &Path) -> Option<SessionMeta> {
    let _span = crate::trace_fn!("kernel::session::identity::load_meta");

    let bytes = fs.read(&meta_path(dir)).ok()?;
    let meta: SessionMeta = serde_json::from_slice(&bytes).ok()?;
    (meta.schema_version == SESSION_SCHEMA_VERSION).then_some(meta)
}

/// Lê o índice temporal (vazio se não existir).
///
/// # Errors
/// [`FsError`] se o ficheiro existir mas for ilegível ou inválido.
pub(super) fn read_index(fs: &dyn Fs, root: &Path) -> Result<Vec<SessionMeta>, FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::read_index");

    let path = index_path(root);
    if !fs.exists(&path) {
        return Ok(Vec::new());
    }
    let bytes = fs.read(&path)?;
    let text = String::from_utf8(bytes).map_err(|err| FsError::Io(err.to_string()))?;
    let mut metas = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let meta: SessionMeta = serde_json::from_str(line).map_err(|err| from_parse(&err))?;
        metas.push(meta);
    }
    Ok(metas)
}

/// Lista as sessões em ordem **temporal** `(created_ms, id)`.
///
/// # Errors
/// [`FsError`] se o índice for ilegível.
pub(super) fn list(fs: &dyn Fs, root: &Path) -> Result<Vec<SessionMeta>, FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::list");

    let mut metas = read_index(fs, root)?;
    metas.sort_by(|left, right| {
        left.created_ms
            .cmp(&right.created_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(metas)
}

/// Procura uma sessão pelo id no índice.
///
/// # Errors
/// [`FsError`] se o índice for ilegível.
pub(super) fn find(
    fs: &dyn Fs,
    root: &Path,
    id: &SessionId,
) -> Result<Option<SessionMeta>, FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::find");

    Ok(list(fs, root)?.into_iter().find(|meta| &meta.id == id))
}

/// Garante a linha de exclusão da auditoria em `.git/info/exclude` (idempotente).
///
/// # Errors
/// [`FsError`] se a escrita falhar.
pub(super) fn ensure_audit_excluded(fs: &dyn Fs, root: &Path) -> Result<(), FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::ensure_audit_excluded");

    let git = root.join(".git");
    if !fs.is_dir(&git) {
        return Ok(());
    }
    let exclude = git.join("info").join("exclude");
    let current = fs.read(&exclude).unwrap_or_default();
    let text = String::from_utf8_lossy(&current);
    if text.lines().any(|line| line.trim() == AUDIT_EXCLUDE_LINE) {
        return Ok(());
    }
    let mut addition = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        addition.push('\n');
    }
    addition.push_str(AUDIT_EXCLUDE_LINE);
    addition.push('\n');
    fs.append(&exclude, addition.as_bytes())
}

/// Anexa uma entrada ao índice temporal.
fn append_index(fs: &dyn Fs, root: &Path, meta: &SessionMeta) -> Result<(), FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::append_index");

    let mut line = to_bytes(meta)?;
    line.push(b'\n');
    fs.append(&index_path(root), &line)
}

fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, FsError> {
    let _span = crate::trace_fn!("kernel::session::identity::to_bytes");

    serde_json::to_vec(value).map_err(|err| FsError::Io(err.to_string()))
}

fn from_parse(err: &serde_json::Error) -> FsError {
    let _span = crate::trace_fn!("kernel::session::identity::from_parse");

    FsError::Io(format!("índice de sessões inválido: {err}"))
}

#[cfg(test)]
mod tests;
