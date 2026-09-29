//! Adaptadores finos das portas (E01-T02).
//!
//! São o **único** sítio do binário que toca o sistema operacional. Os `#[allow]` de
//! `disallowed_methods` são intencionais: a borda é aqui. O log estruturado vive em
//! [`crate::diag`].

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use katu_core::diag::{Level, events};
use katu_core::ports::{
    Clock, Env, ExecRequest, ExecResult, Fs, FsError, Process, ProcessError, Rng, Timestamp,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Relógio do sistema.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o relógio do SO"
    )]
    fn now(&self) -> Timestamp {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => {
                let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                Timestamp::from_millis(millis)
            }
            Err(_) => Timestamp::from_millis(0),
        }
    }
}

/// RNG do sistema (apenas para jitter): *splitmix64* semeado pelo relógio.
pub(crate) struct StdRng {
    state: u64,
}

impl StdRng {
    /// Cria um RNG semeado a partir do relógio do SO.
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: semente do RNG a partir do relógio do SO"
    )]
    pub(crate) fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let seed = u64::try_from(nanos).unwrap_or(0);
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl Rng for StdRng {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ mixed.wrapping_shr(30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ mixed.wrapping_shr(27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ mixed.wrapping_shr(31)
    }
}

/// Sistema de ficheiros real, com escrita atómica (`tmp` → `fsync` → `rename`).
pub(crate) struct StdFs;

impl Fs for StdFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_READ);
        fs::read(path).map_err(|err| FsError::from_io(&err))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_WRITE);
        write_bytes_atomic(path, bytes)
    }

    fn write_atomic_if(&self, path: &Path, bytes: &[u8], expected: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_WRITE);
        let current = fs::read(path).map_err(|err| FsError::from_io(&err))?;
        if current.as_slice() != expected {
            return Err(FsError::Stale);
        }
        // CAS *best-effort*: a janela entre a verificação e o `rename` é reduzida, não eliminada.
        write_bytes_atomic(path, bytes)
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_WRITE);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| FsError::from_io(&err))?;
        file.write_all(bytes)
            .map_err(|err| FsError::from_io(&err))?;
        file.sync_data().map_err(|err| FsError::from_io(&err))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_RENAME);
        fs::rename(from, to).map_err(|err| FsError::from_io(&err))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_MKDIR);
        fs::create_dir_all(path).map_err(|err| FsError::from_io(&err))
    }

    fn mtime(&self, path: &Path) -> Result<Timestamp, FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_STAT);
        let meta = fs::metadata(path).map_err(|err| FsError::from_io(&err))?;
        let modified = meta.modified().map_err(|err| FsError::from_io(&err))?;
        let elapsed = modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| FsError::Io("mtime anterior à epoch".to_string()))?;
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        Ok(Timestamp::from_millis(millis))
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let _span = katu_core::span!(Level::Trace, events::FS_LIST);
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|err| FsError::from_io(&err))?
            .map(|entry| entry.map(|item| item.path()))
            .collect::<Result<Vec<PathBuf>, std::io::Error>>()
            .map_err(|err| FsError::from_io(&err))?;
        entries.sort();
        Ok(entries)
    }
}

/// Caminho do ficheiro temporário para a escrita atómica.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Escrita atómica (`tmp` → `fsync` → `rename`), partilhada pelas variantes de escrita.
fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let temporary = temp_path(path);
    {
        let mut file = File::create(&temporary).map_err(|err| FsError::from_io(&err))?;
        file.write_all(bytes)
            .map_err(|err| FsError::from_io(&err))?;
        file.sync_all().map_err(|err| FsError::from_io(&err))?;
    }
    fs::rename(&temporary, path).map_err(|err| FsError::from_io(&err))
}

/// Ambiente real do processo.
pub(crate) struct StdEnv;

impl Env for StdEnv {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o ambiente do processo"
    )]
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn args(&self) -> Vec<String> {
        std::env::args().skip(1).collect()
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: única porta para o ambiente do processo"
    )]
    fn vars(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
}

/// Processo real: o filho herda o **utilizador** que evocou o katu (sem `sudo`/setuid).
pub(crate) struct StdProcess;

impl Process for StdProcess {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: relógio do SO para o timeout de execução"
    )]
    fn run(&self, request: &ExecRequest) -> Result<ExecResult, ProcessError> {
        let Some((program, args)) = request.argv.split_first() else {
            return Err(ProcessError::Io("argv vazio".to_string()));
        };
        let started = Instant::now();
        let mut command = Command::new(program);
        command.args(args);
        command.current_dir(&request.cwd);
        command.env_clear();
        command.envs(request.env.iter().map(|(key, value)| (key, value)));
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|err| map_spawn_error(&err))?;
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let out_reader = thread::spawn(move || read_stream(&mut out));
        let err_reader = thread::spawn(move || read_stream(&mut err));
        let Some(deadline) = Instant::now().checked_add(Duration::from_millis(request.timeout_ms))
        else {
            return Err(ProcessError::Io("timeout inválido".to_string()));
        };
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|err| map_io_error(&err))? {
                break status;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                drop(child.kill());
                break child.wait().map_err(|err| map_io_error(&err))?;
            }
            thread::sleep(Duration::from_millis(5));
        };
        Ok(ExecResult {
            exit_code: status.code(),
            signal: signal_of(status),
            timed_out,
            duration_ms: elapsed_millis(started.elapsed()),
            stdout: out_reader.join().unwrap_or_default(),
            stderr: err_reader.join().unwrap_or_default(),
        })
    }
}

/// Sinal que matou o processo (unix).
#[cfg(unix)]
fn signal_of(status: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

/// Sinal que matou o processo (não-unix: sempre `None`).
#[cfg(not(unix))]
fn signal_of(_status: std::process::ExitStatus) -> Option<i32> {
    None
}

/// Lê um fluxo até ao fim, devolvendo texto *lossy*.
fn read_stream(stream: &mut Option<impl Read>) -> String {
    let mut bytes = Vec::new();
    if let Some(reader) = stream.as_mut() {
        drop(reader.read_to_end(&mut bytes));
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Mapeia um erro de `spawn` (programa ausente/permissão) para a porta.
fn map_spawn_error(err: &std::io::Error) -> ProcessError {
    match err.kind() {
        std::io::ErrorKind::NotFound => ProcessError::NotFound,
        std::io::ErrorKind::PermissionDenied => ProcessError::Denied,
        _ => ProcessError::Io(err.to_string()),
    }
}

/// Mapeia um erro de I/O do processo para a porta.
fn map_io_error(err: &std::io::Error) -> ProcessError {
    ProcessError::Io(err.to_string())
}

/// Duração em milissegundos, saturante.
fn elapsed_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
