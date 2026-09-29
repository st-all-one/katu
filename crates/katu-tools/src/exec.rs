//! Tool `bash` (E06-T04): execução com `argv`/`cwd` resolvidos, ambiente filtrado e timeout.
//!
//! A política decide **antes** (E02/E07); aqui só se executa o veredicto. Sem regex sobre a string:
//! o `argv` chega resolvido em [`ToolUse::argv`] e a falta dele é **fail-closed**. O processo herda
//! o utilizador que evocou o katu (o adaptador nunca eleva privilégio).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::feedback::{CommandRecord, DEFAULT_TAIL, redact, tail};
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Env, ExecRequest, ExecResult, Process, ProcessError};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

use crate::lang::to_i64;

/// Timeout por omissão (30 s).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Fragmentos que marcam uma variável como sensível (não chega ao filho).
const SECRET_MARKERS: &[&str] = &["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "CREDENTIAL"];

/// Executor de comandos.
pub struct ExecTool<'a> {
    /// Porta de processos.
    pub process: &'a dyn Process,
    /// Porta de ambiente (para filtrar segredos).
    pub env: &'a dyn Env,
    /// Timeout de *wall-clock*.
    pub timeout_ms: u64,
    /// Comando pai, se aninhado (E06-T07).
    pub parent: Option<String>,
}

impl Tool for ExecTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Exec
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_EXEC);
        let ToolArgs::Exec { cwd, .. } = &use_.args else {
            return unavailable("exec");
        };
        let Some(argv) = use_.argv.as_ref() else {
            return unavailable("argv");
        };
        let request = ExecRequest {
            argv: argv.as_slice().to_vec(),
            cwd: Path::new(cwd.as_str()).to_path_buf(),
            env: scrub_env(&self.env.vars()),
            timeout_ms: self.timeout_ms,
        };
        match self.process.run(&request) {
            Ok(result) => {
                let record = build(&request, &result, self.parent.as_deref());
                ToolOutput::report(report(&record))
            }
            Err(ProcessError::NotFound) => unavailable("not-found"),
            Err(ProcessError::Denied) => unavailable("denied"),
            Err(_) => unavailable("exec"),
        }
    }
}

/// Remove variáveis sensíveis do ambiente herdado (E07-T04).
#[must_use]
pub fn scrub_env(vars: &[(String, String)]) -> Vec<(String, String)> {
    vars.iter()
        .filter(|(key, _)| {
            let upper = key.to_ascii_uppercase();
            !SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
        })
        .cloned()
        .collect()
}

/// Constrói o registo do comando (redigido + truncado pela cauda).
fn build(request: &ExecRequest, result: &ExecResult, parent: Option<&str>) -> CommandRecord {
    let seed = format!("exec:{}", request.argv.join(" "));
    CommandRecord {
        id: content_id("x", seed.as_bytes()),
        argv: request.argv.clone(),
        cwd: request.cwd.display().to_string(),
        exit_code: result.exit_code,
        signal: result.signal,
        timed_out: result.timed_out,
        duration_ms: result.duration_ms,
        stdout_tail: tail(&redact(&result.stdout), DEFAULT_TAIL),
        stderr_tail: tail(&redact(&result.stderr), DEFAULT_TAIL),
        parent_command_id: parent.map(str::to_string),
    }
}

/// Renderiza o registo no envelope AI-first (DF12).
fn report(record: &CommandRecord) -> ToolReport {
    let argv: Vec<Value> = record
        .argv
        .iter()
        .map(|arg| Value::str(arg.clone()))
        .collect();
    let data = Value::map(vec![
        ("argv".to_string(), Value::list(argv)),
        ("cwd".to_string(), Value::str(record.cwd.clone())),
        (
            "exit".to_string(),
            Value::int(record.exit_code.map_or(-1, i64::from)),
        ),
        (
            "signal".to_string(),
            Value::int(record.signal.map_or(0, i64::from)),
        ),
        ("timed_out".to_string(), Value::bool(record.timed_out)),
        (
            "duration_ms".to_string(),
            Value::int(to_i64(record.duration_ms)),
        ),
        (
            "stdout".to_string(),
            Value::block(record.stdout_tail.clone()),
        ),
        (
            "stderr".to_string(),
            Value::block(record.stderr_tail.clone()),
        ),
    ]);
    ToolReport::new("exec.run", data)
        .with_id(record.id.clone())
        .with_hash(content_hash(record.id.as_bytes()))
}

fn unavailable(control: &'static str) -> ToolOutput {
    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TIMEOUT_MS, ExecTool, scrub_env};
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::{Tool, ToolOutput};
    use katu_core::ports::{ExecResult, FakeEnv, MemProcess, ProcessError};
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

    fn tool<'a>(process: &'a MemProcess, env: &'a FakeEnv) -> ExecTool<'a> {
        ExecTool {
            process,
            env,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            parent: None,
        }
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
        let output = tool(&process, &env).execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        let rendered = render(&output);
        assert!(rendered.contains("kind: exec.run\n"), "{rendered}");
        assert!(rendered.contains("exit: 0\n"), "{rendered}");
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
        let output = tool(&process, &env).execute(&use_()?);
        let rendered = render(&output);
        assert!(rendered.contains("exit: 2\n"), "{rendered}");
        assert!(rendered.contains("duration_ms: 7\n"), "{rendered}");
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
        let output = tool(&process, &env).execute(&use_()?);
        let rendered = render(&output);
        assert!(rendered.contains("timed_out: true\n"), "{rendered}");
        assert!(rendered.contains("signal: 9\n"), "{rendered}");
        Ok(())
    }

    #[test]
    fn redacts_secret_stdout() -> Result<(), Box<dyn std::error::Error>> {
        let process = MemProcess::ok("MY_SECRET=abc123\n");
        let env = FakeEnv::new();
        let output = tool(&process, &env).execute(&use_()?);
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
        tool(&process, &env).execute(&use_()?);
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
        assert!(matches!(
            tool(&process, &env).execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        Ok(())
    }

    #[test]
    fn missing_argv_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let process = MemProcess::ok("");
        let env = FakeEnv::new();
        let output = tool(&process, &env).execute(&use_with(None)?);
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
}
