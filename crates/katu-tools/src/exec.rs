//! Tool `bash` (E06-T04): execução com `argv`/`cwd` resolvidos, ambiente filtrado e timeout.
//!
//! A política decide **antes** (E02/E07); aqui só se executa o veredicto. Sem regex sobre a string:
//! o `argv` chega resolvido em [`ToolUse::argv`] e a falta dele é **fail-closed**. O processo herda
//! o utilizador que evocou o katu (o adaptador nunca eleva privilégio).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::feedback::{CommandRecord, Ledger, redact};
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{
    Cancel, Env, ExecRequest, ExecResult, Fs, Never, Process, ProcessError, Progress,
};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

use crate::lang::to_i64;

/// Timeout por omissão (30 s).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Diretório de *spill* sob a raiz do workspace (B-04).
const SPILL_DIR: &str = ".katu/spill";

/// Fragmentos que marcam uma variável como sensível (não chega ao filho).
const SECRET_MARKERS: &[&str] = &["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "CREDENTIAL"];

/// Executor de comandos.
pub struct ExecTool<'a> {
    /// Porta de processos.
    pub process: &'a dyn Process,
    /// Porta de ambiente (para filtrar segredos).
    pub env: &'a dyn Env,
    /// Porta de ficheiros (para o *spill* de output grande, B-04).
    pub fs: &'a dyn Fs,
    /// Raiz do workspace (onde fica o diretório de *spill*).
    pub root: &'a Path,
    /// Timeout de *wall-clock*.
    pub timeout_ms: u64,
    /// Comando pai, se aninhado (E06-T07).
    pub parent: Option<String>,
    /// Cancelamento cooperativo do turno (L-P3); `None` = nunca cancela.
    pub cancel: Option<&'a dyn Cancel>,
    /// Progresso efémero do output (`P1/PI_GAINS`); nunca entra no log nem no contexto.
    pub progress: &'a dyn Progress,
}

impl Tool for ExecTool<'_> {
    fn name(&self) -> ToolName {
        let _span = katu_core::trace_fn!("exec::name");

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
        match self
            .process
            .run_streaming(&request, self.cancel.unwrap_or(&Never), &|text: &str| {
                self.progress.chunk("bash", text);
            }) {
            Ok(result) => {
                let record = self.build(&request, &result);
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
    let _span = katu_core::trace_fn!("exec::scrub_env");

    vars.iter()
        .filter(|(key, _)| {
            let upper = key.to_ascii_uppercase();
            !SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
        })
        .cloned()
        .collect()
}

impl ExecTool<'_> {
    /// Constrói o registo do comando (redigido + truncado pelo [`Ledger`], com *spill* se grande).
    fn build(&self, request: &ExecRequest, result: &ExecResult) -> CommandRecord {
        let _span = katu_core::trace_fn!("exec::build");

        let seed = format!("exec:{}", request.argv.join(" "));
        let id = content_id("x", seed.as_bytes());
        let stdout = redact(&result.stdout);
        let stderr = redact(&result.stderr);
        let ledger = Ledger::DEFAULT;
        let (stdout_tail, stdout_spill) = self.render_with_spill(&ledger, &stdout, &id, "stdout");
        let (stderr_tail, stderr_spill) = self.render_with_spill(&ledger, &stderr, &id, "stderr");
        CommandRecord {
            id,
            argv: request.argv.clone(),
            cwd: request.cwd.display().to_string(),
            exit_code: result.exit_code,
            signal: result.signal,
            timed_out: result.timed_out,
            duration_ms: result.duration_ms,
            stdout_tail,
            stderr_tail,
            stdout_spill,
            stderr_spill,
            parent_command_id: self.parent.clone(),
        }
    }

    /// Renderiza o output com o [`Ledger`] e, se exceder o teto, verte-o para uma página de
    /// *spill* sob a raiz do workspace (B-04). Devolve o texto model-visible + o caminho da página.
    ///
    /// O *spill* é best-effort: se a escrita falhar, o texto head/tail mantém-se (sem ponteiro).
    fn render_with_spill(
        &self,
        ledger: &Ledger,
        text: &str,
        id: &str,
        stream: &str,
    ) -> (String, Option<String>) {
        let _span = katu_core::trace_fn!("exec::render_with_spill");

        if text.len() <= ledger.spill_threshold {
            return (ledger.render(text, None), None);
        }
        let spill_path = format!("{SPILL_DIR}/{id}.{stream}");
        let absolute = self.root.join(&spill_path);
        // O spill é redigido (segredos nunca chegam ao ficheiro) e escrito sob a raiz.
        match self.fs.write_atomic(&absolute, text.as_bytes()) {
            Ok(()) => (ledger.render(text, Some(&spill_path)), Some(spill_path)),
            Err(_) => (ledger.render(text, None), None),
        }
    }
}

/// Renderiza o registo no envelope AI-first (DF12).
fn report(record: &CommandRecord) -> ToolReport {
    let _span = katu_core::trace_fn!("exec::report");

    let argv: Vec<Value> = record
        .argv
        .iter()
        .map(|arg| Value::str(arg.clone()))
        .collect();
    let mut entries = vec![
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
    ];
    if let Some(path) = &record.stdout_spill {
        entries.push(("stdout_spill".to_string(), Value::str(path.clone())));
    }
    if let Some(path) = &record.stderr_spill {
        entries.push(("stderr_spill".to_string(), Value::str(path.clone())));
    }
    ToolReport::new("exec.run", Value::map(entries))
        .with_id(record.id.clone())
        .with_hash(content_hash(record.id.as_bytes()))
}

fn unavailable(control: &'static str) -> ToolOutput {
    let _span = katu_core::trace_fn!("exec::unavailable");

    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests;
