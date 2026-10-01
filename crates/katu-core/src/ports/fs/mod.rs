//! Porta de sistema de ficheiros (`Fs`) e a *fake* determinística [`MemFs`].

use std::path::{Path, PathBuf};

use super::clock::Timestamp;

mod mem;

pub use mem::MemFs;

/// Erro de uma operação de sistema de ficheiros.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FsError {
    /// Caminho inexistente.
    NotFound,
    /// O conteúdo mudou desde a leitura (escrita otimista recusada).
    Stale,
    /// Outro erro de I/O (mensagem estável, sem segredo).
    Io(String),
}

impl FsError {
    /// Converte um erro de I/O do `std` num erro de porta.
    #[must_use]
    pub fn from_io(err: &std::io::Error) -> Self {
        let _span = crate::trace_fn!("ports::fs::from_io");

        if err.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound
        } else {
            Self::Io(err.to_string())
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _span = crate::trace_fn!("ports::fs::fmt");

        match self {
            Self::NotFound => f.write_str("caminho não encontrado"),
            Self::Stale => f.write_str("conteúdo mudou desde a leitura"),
            Self::Io(message) => write!(f, "erro de I/O: {message}"),
        }
    }
}

impl std::error::Error for FsError {}

/// Porta de sistema de ficheiros (I/O confinado e testável).
pub trait Fs: Send + Sync {
    /// Lê um ficheiro inteiro.
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError>;

    /// Lê de `offset` (bytes) até ao fim, para retomada incremental (ADR 0008).
    ///
    /// `offset == len` devolve vazio; acima do fim é erro (nunca lê lixo).
    fn read_from(&self, path: &Path, offset: u64) -> Result<Vec<u8>, FsError>;

    /// Resolve **symlinks** e `.`/`..` de `path` (E07-T02), **antes** do veredicto de política.
    ///
    /// A folha pode não existir (ex.: `write` de um ficheiro novo): resolve-se o ancestral
    /// existente e junta-se o nome final. Um ciclo ou profundidade excessiva é `Io` (fail-closed).
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError>;

    /// Escreve atomicamente: temporário → `fsync` → `rename`.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;

    /// Escreve atomicamente **só se** o conteúdo atual for exatamente `expected`.
    ///
    /// É o *compare-and-swap* do `edit` (read-before-write): se outro escritor alterou o ficheiro
    /// entretanto, devolve [`FsError::Stale`] e **não** grava (fail-closed; nunca faz clobber de
    /// uma edição concorrente). Um ficheiro inexistente nunca casa com `expected`.
    fn write_atomic_if(&self, path: &Path, bytes: &[u8], expected: &[u8]) -> Result<(), FsError>;

    /// Anexa bytes ao fim de um ficheiro (cria se não existir), de forma durável.
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;

    /// Anexa bytes **sem** garantir durabilidade: a barreira é feita depois por [`Fs::sync`].
    ///
    /// É o que permite o *group commit* do log (ADR 0024, P-01): o `fsync` passa a acontecer na
    /// fronteira do turno em vez de uma vez por evento. O default **cai em [`Fs::append`]** — um
    /// backend que não saiba adiar continua correto (só mais lento).
    ///
    /// # Errors
    /// Como [`Fs::append`].
    fn append_unsynced(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = crate::trace_fn!("ports::fs::append_unsynced");

        self.append(path, bytes)
    }

    /// Torna durável o que já foi escrito em `path` (barreira do *group commit*).
    ///
    /// O default é **no-op** (nada a fazer num backend sem `fsync`): quem adia tem de o sobrepor.
    ///
    /// # Errors
    /// [`FsError::Io`] se a barreira falhar.
    fn sync(&self, _path: &Path) -> Result<(), FsError> {
        let _span = crate::trace_fn!("ports::fs::sync");

        Ok(())
    }

    /// Move/renomeia atomicamente `from` → `to` (o conteúdo **não** muda).
    ///
    /// Semântica POSIX: substitui `to` se já existir. O executor `move` verifica a existência do
    /// destino antes, para nunca sobrescrever (fail-closed).
    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError>;

    /// Cria um diretório e os seus pais (idempotente).
    fn create_dir_all(&self, path: &Path) -> Result<(), FsError>;

    /// `true` se o caminho existe.
    fn exists(&self, path: &Path) -> bool;

    /// `true` se o caminho é um diretório.
    fn is_dir(&self, path: &Path) -> bool;

    /// Modificação (*mtime*) do caminho.
    fn mtime(&self, path: &Path) -> Result<Timestamp, FsError>;

    /// Lista as entradas diretas de um diretório, em ordem canônica.
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError>;

    /// Remove um ficheiro (ou *symlink*) **permanentemente**. Diretórios são recusados
    /// (fail-closed): a remoção é sempre explícita e nunca recursiva.
    ///
    /// # Errors
    /// [`FsError::NotFound`] se o caminho não existir; [`FsError::Io`] se for um diretório ou a
    /// remoção falhar.
    fn remove(&self, path: &Path) -> Result<(), FsError>;
}

#[cfg(test)]
mod tests;
