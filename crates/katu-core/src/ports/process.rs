//! Porta de execução de processos (`Process`) e a *fake* determinística [`MemProcess`].

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// Pedido de execução: `argv` resolvido, `cwd` fixado e ambiente **já filtrado** de segredos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecRequest {
    /// Argumentos (o primeiro é o programa).
    pub argv: Vec<String>,
    /// Diretório de trabalho.
    pub cwd: PathBuf,
    /// Ambiente herdado (sem segredos).
    pub env: Vec<(String, String)>,
    /// Timeout de *wall-clock* em milissegundos.
    pub timeout_ms: u64,
}

/// Resultado de uma execução. Os outcomes são **ortogonais** (`exit_code`/`signal`/`timed_out`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// Código de saída (`None` se morto por sinal).
    pub exit_code: Option<i32>,
    /// Sinal que matou o processo (unix).
    pub signal: Option<i32>,
    /// `true` se o timeout disparou.
    pub timed_out: bool,
    /// Duração de *wall-clock* em milissegundos.
    pub duration_ms: u64,
    /// `stdout` (UTF-8 *lossy*).
    pub stdout: String,
    /// `stderr` (UTF-8 *lossy*).
    pub stderr: String,
}

/// Erro de execução: o processo **não** chegou a correr.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProcessError {
    /// Programa não encontrado.
    NotFound,
    /// Sem permissão de execução.
    Denied,
    /// Outro erro (mensagem estável, sem segredo).
    Io(String),
}

/// Porta de execução de processos.
///
/// A implementação real corre o filho com o **utilizador que evocou o katu** (sem `sudo`/setuid);
/// a porta nunca eleva privilégio.
pub trait Process: Send + Sync {
    /// Corre o pedido e devolve o resultado.
    fn run(&self, request: &ExecRequest) -> Result<ExecResult, ProcessError>;
}

/// Processo falso e determinístico: regista pedidos e devolve um resultado fixo.
#[derive(Debug)]
pub struct MemProcess {
    result: Result<ExecResult, ProcessError>,
    runs: Mutex<Vec<ExecRequest>>,
}

impl MemProcess {
    /// Cria com o resultado devolvido em cada execução.
    #[must_use]
    pub fn new(result: Result<ExecResult, ProcessError>) -> Self {
        Self {
            result,
            runs: Mutex::new(Vec::new()),
        }
    }

    /// Cria um processo que devolve sucesso com `stdout`.
    #[must_use]
    pub fn ok(stdout: &str) -> Self {
        Self::new(Ok(ExecResult {
            exit_code: Some(0),
            signal: None,
            timed_out: false,
            duration_ms: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }))
    }

    /// Cria um processo que falha sempre com `error`.
    #[must_use]
    pub fn failing(error: ProcessError) -> Self {
        Self::new(Err(error))
    }

    /// Pedidos registados, por ordem.
    #[must_use]
    pub fn runs(&self) -> Vec<ExecRequest> {
        lock(&self.runs).clone()
    }
}

impl Process for MemProcess {
    fn run(&self, request: &ExecRequest) -> Result<ExecResult, ProcessError> {
        lock(&self.runs).push(request.clone());
        self.result.clone()
    }
}

/// Bloqueia um `Mutex`, recuperando o valor mesmo que o lock esteja envenenado.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{ExecRequest, ExecResult, MemProcess, Process, ProcessError};
    use std::path::PathBuf;

    fn request() -> ExecRequest {
        ExecRequest {
            argv: vec!["echo".to_string(), "hi".to_string()],
            cwd: PathBuf::from("/work"),
            env: vec![("PATH".to_string(), "/bin".to_string())],
            timeout_ms: 1_000,
        }
    }

    #[test]
    fn mem_process_records_runs() {
        let process = MemProcess::ok("hi\n");
        let first = process.run(&request());
        let second = process.run(&request());
        assert_eq!(first, second);
        assert_eq!(process.runs().len(), 2);
    }

    #[test]
    fn mem_process_returns_configured_error() {
        let process = MemProcess::failing(ProcessError::NotFound);
        assert_eq!(process.run(&request()), Err(ProcessError::NotFound));
    }

    #[test]
    fn mem_process_exposes_orthogonal_outcomes() {
        let process = MemProcess::new(Ok(ExecResult {
            exit_code: None,
            signal: Some(9),
            timed_out: true,
            duration_ms: 42,
            stdout: String::new(),
            stderr: String::new(),
        }));
        let result = process.run(&request());
        assert!(
            matches!(result, Ok(outcome) if outcome.timed_out && outcome.signal == Some(9) && outcome.duration_ms == 42)
        );
    }
}
