//! Tool `bash` (E06-T04): execução com `argv`/`cwd` resolvidos, ambiente filtrado e timeout.
//!
//! A política decide **antes** (E02/E07); aqui só se executa o veredicto. Sem regex sobre a string:
//! o `argv` chega resolvido em [`ToolUse::argv`] e a falta dele é **fail-closed**. O processo herda
//! o utilizador que evocou o katu (o adaptador nunca eleva privilégio).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Env, ExecRequest, ExecResult, Process, ProcessError};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

/// Timeout por omissão (30 s).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Limite de bytes de `stdout`/`stderr` mostrados.
const MAX_OUTPUT: usize = 16_384;

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
            Ok(result) => ToolOutput::report(build(&request, &result)),
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

fn build(request: &ExecRequest, result: &ExecResult) -> ToolReport {
    let (stdout, out_cut) = clip(&result.stdout);
    let (stderr, err_cut) = clip(&result.stderr);
    let exit = result.exit_code.map_or(-1, i64::from);
    let data = Value::map(vec![
        (
            "argv".to_string(),
            Value::list(
                request
                    .argv
                    .iter()
                    .map(|arg| Value::str(arg.clone()))
                    .collect(),
            ),
        ),
        (
            "cwd".to_string(),
            Value::str(request.cwd.display().to_string()),
        ),
        ("exit".to_string(), Value::int(exit)),
        (
            "signal".to_string(),
            Value::int(result.signal.map_or(0, i64::from)),
        ),
        ("timed_out".to_string(), Value::bool(result.timed_out)),
        ("stdout".to_string(), Value::block(stdout)),
        ("stderr".to_string(), Value::block(stderr)),
        ("truncated".to_string(), Value::bool(out_cut || err_cut)),
    ]);
    let seed = format!("exec:{}", request.argv.join(" "));
    ToolReport::new("exec.run", data)
        .with_id(content_id("x", seed.as_bytes()))
        .with_hash(content_hash(seed.as_bytes()))
}

/// Corta o texto ao limite, num limite de caractere válido.
fn clip(text: &str) -> (String, bool) {
    if text.len() <= MAX_OUTPUT {
        return (text.to_string(), false);
    }
    let mut end = MAX_OUTPUT;
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (format!("{}…", &text[..end]), true)
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
            stdout: String::new(),
            stderr: "boom".to_string(),
        }));
        let env = FakeEnv::new();
        let output = tool(&process, &env).execute(&use_()?);
        let rendered = render(&output);
        assert!(rendered.contains("exit: 2\n"), "{rendered}");
        assert!(rendered.contains("boom"), "{rendered}");
        Ok(())
    }

    #[test]
    fn reports_timeout() -> Result<(), Box<dyn std::error::Error>> {
        let process = MemProcess::new(Ok(ExecResult {
            exit_code: None,
            signal: Some(9),
            timed_out: true,
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
