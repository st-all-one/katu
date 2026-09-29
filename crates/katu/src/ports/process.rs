//! Adaptador `StdProcess`: execução real com timeout e leitura **limitada** (E07-T04).
//!
//! O filho corre com o **utilizador** que evocou o katu (sem `sudo`/setuid) e num **process group**
//! próprio (`process_group(0)`). O `timeout` mata o filho direto e reaproveita-o. Matar o **grupo**
//! inteiro (netos) exige FFI (`rustix`/libc), vedado por `#![forbid(unsafe_code)]`; fica para a jail
//! real (E17). Sem isso, a leitura de `stdout`/`stderr` é limitada por [`READ_GRACE_MS`], para um
//! neto que segure o pipe não bloquear o loop.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use katu_core::ports::{ExecRequest, ExecResult, Process, ProcessError};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Período de polling do timeout.
const POLL_MS: u64 = 5;

/// Espera máxima pelos leitores **depois** de o filho terminar.
const READ_GRACE_MS: u64 = 1_000;

/// Processo real: o filho herda o **utilizador** que evocou o katu (sem `sudo`/setuid).
pub(crate) struct StdProcess;

impl Process for StdProcess {
    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: relógio do SO para o timeout de execução"
    )]
    fn run(&self, request: &ExecRequest) -> Result<ExecResult, ProcessError> {
        let started = Instant::now();
        let mut child = spawn_child(request)?;
        let (out_rx, err_rx) = start_readers(&mut child);
        let (status, timed_out) = wait_with_timeout(&mut child, request.timeout_ms)?;
        let grace = Duration::from_millis(READ_GRACE_MS);
        Ok(ExecResult {
            exit_code: status.code(),
            signal: signal_of(status),
            timed_out,
            duration_ms: elapsed_millis(started.elapsed()),
            stdout: out_rx.recv_timeout(grace).unwrap_or_default(),
            stderr: err_rx.recv_timeout(grace).unwrap_or_default(),
        })
    }
}

/// Arranca o filho com `stdio` fechado/piped e, em unix, no seu próprio process group.
fn spawn_child(request: &ExecRequest) -> Result<Child, ProcessError> {
    let Some((program, args)) = request.argv.split_first() else {
        return Err(ProcessError::Io("argv vazio".to_string()));
    };
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
    command.spawn().map_err(|err| map_spawn_error(&err))
}

/// Leitores **destacados**: entregam o texto por canal, para o `run` nunca bloquear num neto.
fn start_readers(child: &mut Child) -> (Receiver<String>, Receiver<String>) {
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    drop(thread::spawn(move || {
        drop(out_tx.send(read_stream(&mut out)));
    }));
    drop(thread::spawn(move || {
        drop(err_tx.send(read_stream(&mut err)));
    }));
    (out_rx, err_rx)
}

/// Espera pelo filho com deadline; no timeout mata e reaproveita o filho (sem zombie).
#[allow(
    clippy::disallowed_methods,
    reason = "adaptador: relógio do SO para o timeout de execução"
)]
fn wait_with_timeout(
    child: &mut Child,
    timeout_ms: u64,
) -> Result<(ExitStatus, bool), ProcessError> {
    let Some(deadline) = Instant::now().checked_add(Duration::from_millis(timeout_ms)) else {
        return Err(ProcessError::Io("timeout inválido".to_string()));
    };
    loop {
        if let Some(status) = child.try_wait().map_err(|err| map_io_error(&err))? {
            return Ok((status, false));
        }
        if Instant::now() >= deadline {
            drop(child.kill());
            let status = child.wait().map_err(|err| map_io_error(&err))?;
            return Ok((status, true));
        }
        thread::sleep(Duration::from_millis(POLL_MS));
    }
}

/// Sinal que matou o processo (unix).
#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

/// Sinal que matou o processo (não-unix: sempre `None`).
#[cfg(not(unix))]
fn signal_of(_status: ExitStatus) -> Option<i32> {
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

#[cfg(all(test, unix))]
mod tests {
    use super::StdProcess;
    use katu_core::ports::{ExecRequest, Process, ProcessError};
    use std::path::PathBuf;

    fn request(argv: &[&str], timeout_ms: u64) -> ExecRequest {
        ExecRequest {
            argv: argv.iter().map(|arg| (*arg).to_string()).collect(),
            cwd: PathBuf::from("/tmp"),
            env: Vec::new(),
            timeout_ms,
        }
    }

    #[test]
    fn normal_command_returns_stdout() -> Result<(), Box<dyn std::error::Error>> {
        let result = StdProcess
            .run(&request(&["/bin/echo", "hi"], 5_000))
            .map_err(|err| format!("run: {err:?}"))?;
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert_eq!(result.stdout, "hi\n");
        Ok(())
    }

    #[test]
    fn timeout_kills_the_direct_child() -> Result<(), Box<dyn std::error::Error>> {
        let result = StdProcess
            .run(&request(&["/bin/sleep", "30"], 100))
            .map_err(|err| format!("run: {err:?}"))?;
        assert!(result.timed_out, "o timeout tem de disparar");
        assert_eq!(result.signal, Some(9), "SIGKILL ao filho direto");
        assert!(result.duration_ms < 10_000, "não pode bloquear 30s");
        Ok(())
    }

    #[test]
    fn missing_program_is_not_found() {
        let result = StdProcess.run(&request(&["/nonexistent/prog"], 1_000));
        assert_eq!(result, Err(ProcessError::NotFound));
    }
}
