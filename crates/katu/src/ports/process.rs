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

/// Tamanho de cada leitura incremental do `stdout`/`stderr` (`P1/PI_GAINS`).
const CHUNK_BYTES: usize = 4_096;

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

    #[allow(
        clippy::disallowed_methods,
        reason = "adaptador: relógio do SO para o timeout de execução"
    )]
    fn run_streaming(
        &self,
        request: &ExecRequest,
        cancel: &dyn Cancel,
        on_chunk: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<ExecResult, ProcessError> {
        let _span = katu_core::trace_fn!("ports::process::run_streaming");

        let started = Instant::now();
        let mut child = spawn_child(request)?;
        let (out_rx, err_rx) = start_streaming_readers(&mut child);
        let mut out_text = String::new();
        let mut err_text = String::new();
        let (status, timed_out) = wait_streaming(
            &mut child,
            request.timeout_ms,
            cancel,
            &out_rx,
            &err_rx,
            on_chunk,
            &mut out_text,
            &mut err_text,
        )?;
        // Grace: drena o que ficou nos canais (um neto pode segurar o `stdout`).
        let grace = Duration::from_millis(READ_GRACE_MS);
        drain_streaming(&out_rx, grace, on_chunk, &mut out_text);
        drain_streaming(&err_rx, grace, on_chunk, &mut err_text);
        Ok(ExecResult {
            exit_code: status.code(),
            signal: signal_of(status),
            timed_out,
            duration_ms: elapsed_millis(started.elapsed()),
            stdout: out_text,
            stderr: err_text,
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

/// Leitores **destacados** que entregam cada fragmento por canal (`P1/PI_GAINS`).
fn start_streaming_readers(child: &mut Child) -> (Receiver<String>, Receiver<String>) {
    let _span = katu_core::trace_fn!("ports::process::start_streaming_readers");

    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    drop(thread::spawn(move || read_streaming(&mut out, &out_tx)));
    drop(thread::spawn(move || read_streaming(&mut err, &err_tx)));
    (out_rx, err_rx)
}

/// Lê um fluxo em fragmentos, enviando cada um pelo canal (para o *streaming* em tempo real).
fn read_streaming(stream: &mut Option<impl Read>, tx: &mpsc::Sender<String>) {
    let _span = katu_core::trace_fn!("ports::process::read_streaming");

    let mut buf = [0u8; CHUNK_BYTES];
    if let Some(reader) = stream.as_mut() {
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text =
                        String::from_utf8_lossy(buf.get(..n).unwrap_or_default()).into_owned();
                    if tx.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

/// Espera pelo filho drenando os fragmentos em tempo real (timeout/cancelamento matam o grupo).
#[allow(
    clippy::disallowed_methods,
    reason = "adaptador: relógio do SO para o timeout de execução"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "o passo de streaming recebe os dois canais, o observador e os dois acumuladores — eixos distintos do mesmo laço"
)]
fn wait_streaming(
    child: &mut Child,
    timeout_ms: u64,
    cancel: &dyn Cancel,
    out_rx: &Receiver<String>,
    err_rx: &Receiver<String>,
    on_chunk: &(dyn Fn(&str) + Send + Sync),
    out_text: &mut String,
    err_text: &mut String,
) -> Result<(ExitStatus, bool), ProcessError> {
    let _span = katu_core::trace_fn!("ports::process::wait_streaming");

    let Some(deadline) = Instant::now().checked_add(Duration::from_millis(timeout_ms)) else {
        return Err(ProcessError::Io("timeout inválido".to_string()));
    };
    loop {
        drain_ready(out_rx, on_chunk, out_text);
        drain_ready(err_rx, on_chunk, err_text);
        if let Some(status) = child.try_wait().map_err(|err| map_io_error(&err))? {
            return Ok((status, false));
        }
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

/// Drena os fragmentos já disponíveis, sem bloquear (chamado no laço de espera).
fn drain_ready(rx: &Receiver<String>, on_chunk: &(dyn Fn(&str) + Send + Sync), text: &mut String) {
    let _span = katu_core::trace_fn!("ports::process::drain_ready");

    while let Ok(chunk) = rx.try_recv() {
        on_chunk(&chunk);
        text.push_str(&chunk);
    }
}

/// Drena o que resta nos canais com um prazo (grace após o filho terminar).
#[allow(
    clippy::disallowed_methods,
    reason = "adaptador: relógio do SO para o grace de leitura"
)]
fn drain_streaming(
    rx: &Receiver<String>,
    grace: Duration,
    on_chunk: &(dyn Fn(&str) + Send + Sync),
    text: &mut String,
) {
    let _span = katu_core::trace_fn!("ports::process::drain_streaming");

    let Some(deadline) = Instant::now().checked_add(grace) else {
        return;
    };
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(chunk) => {
                on_chunk(&chunk);
                text.push_str(&chunk);
            }
            Err(_) => break,
        }
    }
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
mod tests;
