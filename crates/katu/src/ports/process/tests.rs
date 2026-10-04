use super::StdProcess;
use katu_core::ports::{ExecRequest, Never, Process, ProcessError, Progress};
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

/// Observador que acumula os fragmentos de output.
#[derive(Default)]
struct Recorder {
    chunks: std::sync::Mutex<Vec<String>>,
}

impl Progress for Recorder {
    fn chunk(&self, _name: &str, text: &str) {
        if let Ok(mut chunks) = self.chunks.lock() {
            chunks.push(text.to_string());
        }
    }
}

/// `run_streaming` entrega o output em fragmentos **e** devolve-o completo (`P1/PI_GAINS`).
#[test]
fn streaming_collects_output_and_delivers_chunks() -> Result<(), Box<dyn std::error::Error>> {
    let recorder = Recorder::default();
    let result = StdProcess
        .run_streaming(
            &request(&["/bin/sh", "-c", "printf 'um\\ndois\\n'"], 5_000),
            &Never,
            &|text: &str| recorder.chunk("bash", text),
        )
        .map_err(|err| format!("run: {err:?}"))?;
    assert_eq!(result.stdout, "um\ndois\n");
    let chunks = recorder
        .chunks
        .lock()
        .map(|chunks| chunks.concat())
        .unwrap_or_default();
    assert_eq!(chunks, "um\ndois\n");
    Ok(())
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
