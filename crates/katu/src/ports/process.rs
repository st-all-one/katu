//! Adaptador `StdProcess`: execução real com timeout e leitura **limitada** (E07-T04).
//!
//! O filho corre com o **utilizador** que evocou o katu (sem `sudo`/setuid) e num **process group**
//! próprio (`process_group(0)`). No timeout, o grupo inteiro (filho + netos) é morto com
//! `kill(-pgid, SIGKILL)` — o **único** ponto `unsafe` do projeto (ADR 0016/E07-T04), isolado em
//! [`kill_group`] com a fronteira de segurança documentada. Sem isso, um neto que segure o `stdout`
//! sobreviveria ao timeout; a leitura é ainda limitada por [`READ_GRACE_MS`].

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use katu_core::diag::{Level, events};
use katu_core::ports::{Cancel, ExecRequest, ExecResult, Process, ProcessError};

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
    fn run(&self, request: &ExecRequest, cancel: &dyn Cancel) -> Result<ExecResult, ProcessError> {
        let _span = katu_core::trace_fn!("ports::process::run");

        let started = Instant::now();
        let mut child = spawn_child(request)?;
        let (out_rx, err_rx) = start_readers(&mut child);
        let (status, timed_out) = wait_with_timeout(&mut child, request.timeout_ms, cancel)?;
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
    let _span = katu_core::trace_fn!("ports::process::spawn_child");

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
    let _span = katu_core::trace_fn!("ports::process::start_readers");

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

/// Espera pelo filho com deadline; no timeout **ou cancelamento** mata o **grupo** e reaproveita o
/// filho (sem zombie).
#[allow(
    clippy::disallowed_methods,
    reason = "adaptador: relógio do SO para o timeout de execução"
)]
fn wait_with_timeout(
    child: &mut Child,
    timeout_ms: u64,
    cancel: &dyn Cancel,
) -> Result<(ExitStatus, bool), ProcessError> {
    let _span = katu_core::trace_fn!("ports::process::wait_with_timeout");

    let Some(deadline) = Instant::now().checked_add(Duration::from_millis(timeout_ms)) else {
        return Err(ProcessError::Io("timeout inválido".to_string()));
    };
    loop {
        if let Some(status) = child.try_wait().map_err(|err| map_io_error(&err))? {
            return Ok((status, false));
        }
        // L-P3: o cancelamento do turno interrompe o `bash` longo (mata o grupo, como o timeout).
        if cancel.cancelled() {
            kill_group(child);
            drop(child.kill());
            let status = child.wait().map_err(|err| map_io_error(&err))?;
            return Ok((status, true));
        }
        if Instant::now() >= deadline {
            kill_group(child);
            drop(child.kill());
            let status = child.wait().map_err(|err| map_io_error(&err))?;
            return Ok((status, true));
        }
        thread::sleep(Duration::from_millis(POLL_MS));
    }
}

/// Mata o **grupo de processos** do filho (o filho e os netos), em unix.
///
/// O filho é líder do seu próprio grupo (`process_group(0)`, logo pgid = pid do filho); enviar
/// `SIGKILL` ao grupo (`kill(-pgid)`) atinge o filho e os netos. Em não-unix é um no-op: o filho
/// direto continua a ser morto por `Child::kill`.
#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "kill(2) com pid negativo (grupo) não tem wrapper em std; único `unsafe` do projeto (ADR 0016/E07-T04)"
)]
fn kill_group(child: &Child) {
    let _span = katu_core::trace_fn!("ports::process::kill_group");

    let Ok(pid) = i32::try_from(child.id()) else {
        return;
    };
    let Some(group) = pid.checked_neg() else {
        return;
    };
    // SAFETY: `pid` vem de `Child::id()` (o líder do grupo criado por `process_group(0)`); `-pid`
    // refere **apenas** esse grupo, criado por nós nesta chamada — nunca um grupo alheio. O retorno
    // é ignorado de propósito (`ESRCH` = já morreu).
    let _status: i32 = unsafe { libc::kill(group, libc::SIGKILL) };
    katu_core::event!(Level::Warn, events::PROCESS_KILL, "pgid" => pid);
}

/// Não-unix: sem process group; o filho direto é morto por `Child::kill`.
#[cfg(not(unix))]
fn kill_group(_child: &Child) {
    let _span = katu_core::trace_fn!("ports::process::kill_group");
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
    let _span = katu_core::trace_fn!("ports::process::signal_of");

    None
}

/// Lê um fluxo até ao fim, devolvendo texto *lossy*.
fn read_stream(stream: &mut Option<impl Read>) -> String {
    let _span = katu_core::trace_fn!("ports::process::read_stream");

    let mut bytes = Vec::new();
    if let Some(reader) = stream.as_mut() {
        drop(reader.read_to_end(&mut bytes));
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Mapeia um erro de `spawn` (programa ausente/permissão) para a porta.
fn map_spawn_error(err: &std::io::Error) -> ProcessError {
    let _span = katu_core::trace_fn!("ports::process::map_spawn_error");

    match err.kind() {
        std::io::ErrorKind::NotFound => ProcessError::NotFound,
        std::io::ErrorKind::PermissionDenied => ProcessError::Denied,
        _ => ProcessError::Io(err.to_string()),
    }
}

/// Mapeia um erro de I/O do processo para a porta.
fn map_io_error(err: &std::io::Error) -> ProcessError {
    let _span = katu_core::trace_fn!("ports::process::map_io_error");

    ProcessError::Io(err.to_string())
}

/// Duração em milissegundos, saturante.
fn elapsed_millis(duration: Duration) -> u64 {
    let _span = katu_core::trace_fn!("ports::process::elapsed_millis");

    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(all(test, unix))]
mod tests {
    use super::StdProcess;
    use katu_core::ports::{ExecRequest, Never, Process, ProcessError};
    use std::path::PathBuf;

    fn request(argv: &[&str], timeout_ms: u64) -> ExecRequest {
        let _span = katu_core::trace_fn!("ports::process::request");

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
            .run(&request(&["/bin/echo", "hi"], 5_000), &Never)
            .map_err(|err| format!("run: {err:?}"))?;
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert_eq!(result.stdout, "hi\n");
        Ok(())
    }

    #[test]
    fn timeout_kills_the_direct_child() -> Result<(), Box<dyn std::error::Error>> {
        let result = StdProcess
            .run(&request(&["/bin/sleep", "30"], 100), &Never)
            .map_err(|err| format!("run: {err:?}"))?;
        assert!(result.timed_out, "o timeout tem de disparar");
        assert_eq!(result.signal, Some(9), "SIGKILL ao filho direto");
        assert!(result.duration_ms < 10_000, "não pode bloquear 30s");
        Ok(())
    }

    #[test]
    fn timeout_kills_the_process_group() -> Result<(), Box<dyn std::error::Error>> {
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("katu-pgid-{}", std::process::id()));
        drop(std::fs::create_dir_all(&dir));
        let marker = dir.join("late.txt");
        drop(std::fs::remove_file(&marker));
        // O neto dorme 2s e só depois escreve o marcador. Se o grupo for morto no timeout (~300ms),
        // o neto nunca chega a escrever; se só o filho direto morrer, o neto sobrevive e escreve.
        let script = "( sleep 2; echo late > \"$1\" ) & sleep 30";
        let path = marker.to_str().ok_or("caminho inválido")?;
        let result = StdProcess
            .run(
                &request(&["/bin/sh", "-c", script, "sh", path], 300),
                &Never,
            )
            .map_err(|err| format!("run: {err:?}"))?;
        assert!(result.timed_out, "o timeout tem de disparar");
        std::thread::sleep(Duration::from_millis(2_500));
        assert!(
            !marker.exists(),
            "o neto sobreviveu ao timeout (grupo não morto)"
        );
        drop(std::fs::remove_dir_all(&dir));
        Ok(())
    }

    #[test]
    fn missing_program_is_not_found() {
        let result = StdProcess.run(&request(&["/nonexistent/prog"], 1_000), &Never);
        assert_eq!(result, Err(ProcessError::NotFound));
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "o teste mede o tempo real do cancelamento (L-P3)"
    )]
    fn cancel_kills_the_running_child() -> Result<(), Box<dyn std::error::Error>> {
        use katu_core::ports::Flag;
        use std::time::Duration;
        let flag = Flag::new();
        let signal = flag.clone();
        drop(std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            signal.request();
        }));
        let started = std::time::Instant::now();
        let result = StdProcess
            .run(&request(&["/bin/sleep", "30"], 10_000), &flag)
            .map_err(|err| format!("run: {err:?}"))?;
        assert!(result.timed_out, "o cancelamento interrompe o filho");
        assert!(
            started.elapsed() < Duration::from_millis(5_000),
            "não espera os 30s do comando"
        );
        Ok(())
    }
}
