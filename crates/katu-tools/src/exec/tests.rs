use super::{DEFAULT_TIMEOUT_MS, ExecTool, scrub_env};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{
    Cancel, ExecRequest, ExecResult, FakeEnv, MemFs, MemProcess, NO_PROGRESS, Process,
    ProcessError, Progress,
};
use katu_core::report::ToolReport;
use katu_policy::{ResolvedArgv, ResolvedPath, ToolArgs, ToolName, ToolUse};

fn use_with(argv: Option<&[&str]>) -> Result<ToolUse, katu_policy::PolicyError> {
    let cwd = ResolvedPath::from_canonical("/work")?;
    let resolved = ResolvedArgv::new(
        argv.unwrap_or(&["echo", "hi"])
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
    )?;
    Ok(ToolUse {
        name: ToolName::Exec,
        args: ToolArgs::Exec {
            argv: resolved.clone(),
            cwd: cwd.clone(),
        },
        resolved_paths: vec![cwd.clone()],
        argv: argv.map(|_| resolved),
        cwd,
    })
}

fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
    use_with(Some(&["echo", "hi"]))
}

fn tool<'a>(process: &'a MemProcess, env: &'a FakeEnv, fs: &'a MemFs) -> ExecTool<'a> {
    ExecTool {
        process,
        env,
        fs,
        root: std::path::Path::new("/work"),
        timeout_ms: DEFAULT_TIMEOUT_MS,
        parent: None,
        cancel: None,
        progress: &NO_PROGRESS,
    }
}

/// Processo falso que emite fragmentos em *streaming* (`P1/PI_GAINS`).
struct ChunkProcess;

impl Process for ChunkProcess {
    fn run(
        &self,
        _request: &ExecRequest,
        _cancel: &dyn Cancel,
    ) -> Result<ExecResult, ProcessError> {
        Ok(ExecResult {
            exit_code: Some(0),
            signal: None,
            timed_out: false,
            duration_ms: 0,
            stdout: "um\ndois\n".to_string(),
            stderr: String::new(),
        })
    }

    fn run_streaming(
        &self,
        request: &ExecRequest,
        cancel: &dyn Cancel,
        on_chunk: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<ExecResult, ProcessError> {
        on_chunk("um\n");
        on_chunk("dois\n");
        self.run(request, cancel)
    }
}

/// Observador que regista os fragmentos recebidos.
#[derive(Default)]
struct Recorder {
    chunks: std::sync::Mutex<Vec<String>>,
}

impl Progress for Recorder {
    fn chunk(&self, name: &str, text: &str) {
        if let Ok(mut chunks) = self.chunks.lock() {
            chunks.push(format!("{name}:{text}"));
        }
    }
}

/// O output incremental da tool chega ao observador **sem** alterar o envelope final.
#[test]
fn streaming_delivers_tool_output_chunks() -> Result<(), Box<dyn std::error::Error>> {
    let process = ChunkProcess;
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let recorder = Recorder::default();
    let tool = ExecTool {
        process: &process,
        env: &env,
        fs: &fs,
        root: std::path::Path::new("/work"),
        timeout_ms: DEFAULT_TIMEOUT_MS,
        parent: None,
        cancel: None,
        progress: &recorder,
    };
    let output = tool.execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    assert!(render(&output).contains("um\ndois\n"));
    let chunks = recorder
        .chunks
        .lock()
        .map(|chunks| chunks.clone())
        .unwrap_or_default();
    assert_eq!(
        chunks,
        vec!["bash:um\n".to_string(), "bash:dois\n".to_string()]
    );
    Ok(())
}

fn render(output: &ToolOutput) -> String {
    output
        .report
        .as_ref()
        .map_or_else(String::new, ToolReport::to_toon)
}

#[test]
fn runs_and_reports() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::ok("hi\n");
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    let rendered = render(&output);
    assert!(rendered.contains("exec.run\u{1f}"));
    assert!(rendered.contains("exit\u{1f}0\nsignal\u{1f}0\ntimed_out\u{1f}0\n"));
    assert!(rendered.contains("hi\n"), "{rendered}");
    Ok(())
}

#[test]
fn reports_non_zero_exit() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::new(Ok(ExecResult {
        exit_code: Some(2),
        signal: None,
        timed_out: false,
        duration_ms: 7,
        stdout: String::new(),
        stderr: "boom".to_string(),
    }));
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    let rendered = render(&output);
    assert!(
        rendered.contains("exit\u{1f}2\nsignal\u{1f}0\ntimed_out\u{1f}0\nduration_ms\u{1f}7\n")
    );
    assert!(rendered.contains("boom"), "{rendered}");
    Ok(())
}

#[test]
fn reports_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::new(Ok(ExecResult {
        exit_code: None,
        signal: Some(9),
        timed_out: true,
        duration_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
    }));
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    let rendered = render(&output);
    assert!(rendered.contains("exit\u{1f}-1\nsignal\u{1f}9\ntimed_out\u{1f}1\n"));
    Ok(())
}

#[test]
fn redacts_secret_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::ok("MY_SECRET=abc123\n");
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    let rendered = render(&output);
    assert!(!rendered.contains("abc123"), "{rendered}");
    assert!(rendered.contains("[redacted]"), "{rendered}");
    Ok(())
}

#[test]
fn scrubs_secret_env_before_running() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::ok("");
    let env = FakeEnv::new()
        .with_var("PATH", "/bin")
        .with_var("MY_SECRET", "s3cr3t")
        .with_var("OPENAI_API_KEY", "sk-x");
    let fs = MemFs::new();
    tool(&process, &env, &fs).execute(&use_()?);
    let runs = process.runs();
    let recorded = runs.first().ok_or("sem execução")?;
    assert!(recorded.env.iter().any(|(key, _)| key == "PATH"));
    assert!(!recorded.env.iter().any(|(key, _)| key == "MY_SECRET"));
    assert!(!recorded.env.iter().any(|(key, _)| key == "OPENAI_API_KEY"));
    Ok(())
}

#[test]
fn missing_program_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::failing(ProcessError::NotFound);
    let env = FakeEnv::new();
    let fs = MemFs::new();
    assert!(matches!(
        tool(&process, &env, &fs).execute(&use_()?).outcome,
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn missing_argv_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::ok("");
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_with(None)?);
    assert!(matches!(output.outcome, ToolOutcome::Unavailable { .. }));
    assert!(process.runs().is_empty());
    Ok(())
}

#[test]
fn scrub_keeps_ordinary_vars() {
    let vars = vec![
        ("PATH".to_string(), "/bin".to_string()),
        ("HOME".to_string(), "/home/me".to_string()),
        ("GITHUB_TOKEN".to_string(), "x".to_string()),
    ];
    let scrubbed = scrub_env(&vars);
    assert_eq!(scrubbed.len(), 2);
    assert!(scrubbed.iter().all(|(key, _)| key != "GITHUB_TOKEN"));
}

#[test]
fn large_output_is_spilled_with_a_pointer() -> Result<(), Box<dyn std::error::Error>> {
    // Output acima do teto do Ledger: é vertido para uma página de spill e o ponteiro é
    // incluído no texto model-visible (B-04).
    let big = "x".repeat(10_000);
    let process = MemProcess::ok(&big);
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    let rendered = render(&output);
    assert!(rendered.contains("stdout_spill\u{1f}"), "{rendered}");
    assert!(rendered.contains(".katu/spill/"), "{rendered}");
    // O texto model-visible não contém o output inteiro (head/tail + ponteiro).
    assert!(!rendered.contains(&big), "{rendered}");
    Ok(())
}

#[test]
fn small_output_is_not_spilled() -> Result<(), Box<dyn std::error::Error>> {
    let process = MemProcess::ok("pequeno\n");
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let output = tool(&process, &env, &fs).execute(&use_()?);
    let rendered = render(&output);
    assert!(!rendered.contains("stdout_spill"), "{rendered}");
    assert!(rendered.contains("pequeno"), "{rendered}");
    Ok(())
}
