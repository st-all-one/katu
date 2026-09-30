//! Resolução de `katu run`/`katu tui`: `--params` (XOR flags), `--batch` JSONL e defaults da
//! config (E20-T08/T17).
//!
//! Sem inferência: `--params` e flags explícitas **não** coexistem (exit 2); um item de lote sem
//! `body` recusa; um lote inválido recusa **antes** de executar seja o que for.

use katu_core::error::Error;
use katu_core::provider::Thinking;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::defaults;
use crate::defaults::Defaults;
use crate::report::Report;

use super::input;
use super::params;
use super::run_cmd::{RunCli, TuiCli, run_once};

/// Parâmetros de uma rodada (JSON de `--params` ou de uma linha de `--batch`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunParams {
    /// Objetivo (alternativa ao posicional).
    pub(crate) body: Option<String>,
    /// Provider.
    pub(crate) provider: Option<String>,
    /// Modelo.
    pub(crate) model: Option<String>,
    /// Base URL.
    pub(crate) base: Option<String>,
    /// Grau de pensamento.
    pub(crate) thinking: Option<String>,
    /// Teto de tokens de saída.
    pub(crate) max_tokens: Option<u32>,
    /// Máximo de passos por turno.
    pub(crate) max_steps: Option<u32>,
    /// Compactação do histórico.
    pub(crate) compact: Option<bool>,
    /// Retomada de sessão.
    pub(crate) resume: Option<String>,
}

/// Configuração resolvida de uma rodada.
#[derive(Debug, Clone)]
pub(crate) struct RunConfig {
    /// Objetivo.
    pub(crate) goal: String,
    /// Provider.
    pub(crate) provider: String,
    /// Modelo.
    pub(crate) model: Option<String>,
    /// Base URL.
    pub(crate) base: Option<String>,
    /// Grau de pensamento resolvido.
    pub(crate) thinking: Option<Thinking>,
    /// Teto de tokens de saída.
    pub(crate) max_tokens: u32,
    /// Máximo de passos por turno.
    pub(crate) max_steps: u32,
    /// Compactação do histórico.
    pub(crate) compact: bool,
    /// Retomada de sessão.
    pub(crate) resume: Option<String>,
}

/// Flags explícitas de uma rodada (para o XOR com `--params`).
#[derive(Debug, Default)]
struct Flags {
    provider: Option<String>,
    model: Option<String>,
    base: Option<String>,
    thinking: Option<String>,
    max_tokens: Option<u32>,
    max_steps: Option<u32>,
    compact: Option<bool>,
    resume: Option<String>,
}

impl Flags {
    /// Flags explícitas de `katu run`.
    fn from_run(args: &RunCli) -> Self {
        Self {
            provider: args.provider.clone(),
            model: args.model.clone(),
            base: args.base.clone(),
            thinking: args.thinking.clone(),
            max_tokens: args.max_tokens,
            max_steps: args.max_steps,
            compact: args.compact,
            resume: args.resume.clone(),
        }
    }

    /// Flags explícitas de `katu tui`.
    fn from_tui(args: &TuiCli) -> Self {
        Self {
            provider: args.provider.clone(),
            model: args.model.clone(),
            base: args.base.clone(),
            thinking: args.thinking.clone(),
            max_tokens: args.max_tokens,
            max_steps: args.max_steps,
            compact: args.compact,
            resume: args.resume.clone(),
        }
    }

    /// Se alguma flag explícita foi usada.
    fn any(&self) -> bool {
        self.provider.is_some()
            || self.model.is_some()
            || self.base.is_some()
            || self.thinking.is_some()
            || self.max_tokens.is_some()
            || self.max_steps.is_some()
            || self.compact.is_some()
            || self.resume.is_some()
    }

    /// Combina flags, `--params` e defaults (flags > params > config > default do comando).
    fn assemble(
        self,
        goal: String,
        parsed: RunParams,
        defaults: &Defaults,
    ) -> Result<RunConfig, Error> {
        let RunParams {
            body: _,
            provider,
            model,
            base,
            thinking,
            max_tokens,
            max_steps,
            compact,
            resume,
        } = parsed;
        let thinking = self
            .thinking
            .or(thinking)
            .or_else(|| defaults.thinking.clone());
        let thinking = match thinking {
            Some(raw) => Some(Thinking::parse(&raw).ok_or_else(|| {
                Error::invalid_input(format!("thinking inválido: `{raw}` (off/low/medium/high)"))
            })?),
            None => None,
        };
        Ok(RunConfig {
            goal,
            provider: self
                .provider
                .or(provider)
                .or_else(|| defaults.provider.clone())
                .unwrap_or_else(|| "llama".to_owned()),
            model: self.model.or(model).or_else(|| defaults.model.clone()),
            base: self.base.or(base).or_else(|| defaults.base.clone()),
            thinking,
            max_tokens: self.max_tokens.or(max_tokens).unwrap_or(512),
            max_steps: self.max_steps.or(max_steps).unwrap_or(8),
            compact: self
                .compact
                .or(compact)
                .or(defaults.auto_compact)
                .unwrap_or(false),
            resume: self.resume.or(resume),
        })
    }
}

/// Executa `katu run` (uma rodada ou um lote).
pub(crate) fn execute(args: &RunCli) -> Report {
    if let Some(path) = &args.batch {
        return batch(args, path);
    }
    match resolve(args) {
        Ok(config) => run_once(&config),
        Err(error) => Report::failed("run", &error),
    }
}

/// Resolve a config do `katu tui` (mesmas regras de `run`, sem body nem lote).
pub(crate) fn resolve_tui(args: &TuiCli) -> Result<RunConfig, Error> {
    let flags = Flags::from_tui(args);
    let parsed = parse_params(args.params.as_deref(), flags.any())?;
    flags.assemble("tui".to_owned(), parsed, &defaults::current())
}

/// Resolve uma rodada a partir das flags e/ou de `--params` (XOR).
fn resolve(args: &RunCli) -> Result<RunConfig, Error> {
    let flags = Flags::from_run(args);
    let parsed = parse_params(args.params.as_deref(), flags.any())?;
    if args.body.is_some() && parsed.body.is_some() {
        return Err(Error::invalid_input(
            "body posicional e `body` em --params são exclusivos",
        ));
    }
    let goal = match &args.body {
        Some(body) => input::resolve(Some(body))?,
        None => match parsed.body.clone() {
            Some(body) => body,
            None => input::resolve(None)?,
        },
    };
    flags.assemble(goal, parsed, &defaults::current())
}

/// Lê `--params` (recusando a coexistência com flags explícitas).
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`explicit` é o lado do XOR com `--params`"
)]
fn parse_params(raw: Option<&str>, explicit: bool) -> Result<RunParams, Error> {
    if raw.is_some() && explicit {
        return Err(Error::invalid_input(
            "--params é exclusivo com flags explícitas (não se infere)",
        ));
    }
    match raw {
        Some(raw) => params::parse(&params::source(raw)?),
        None => Ok(RunParams::default()),
    }
}

/// Processa um lote JSONL: valida tudo **antes** de executar.
fn batch(args: &RunCli, path: &str) -> Report {
    if args.params.is_some() || Flags::from_run(args).any() || args.body.is_some() {
        return Report::failed(
            "run",
            &Error::invalid_input("--batch é exclusivo com --params, flags e body"),
        );
    }
    let defaults = defaults::current();
    let lines = match params::batch_lines(path) {
        Ok(lines) => lines,
        Err(error) => return Report::failed("run", &error),
    };
    let mut configs = Vec::with_capacity(lines.len());
    for line in &lines {
        let parsed: RunParams = match params::parse(line) {
            Ok(parsed) => parsed,
            Err(error) => return Report::failed("run", &error),
        };
        match config_from_params(parsed, &defaults) {
            Ok(config) => configs.push(config),
            Err(error) => return Report::failed("run", &error),
        }
    }
    let mut items = Vec::with_capacity(configs.len());
    let mut exit = 0_u8;
    for config in &configs {
        let report = run_once(config);
        if !report.success {
            exit = report.exit;
        }
        items.push(report.data.unwrap_or(Value::Null));
    }
    let mut report = Report::ok("run", Some(json!({ "items": items })));
    report.success = exit == 0;
    report.exit = exit;
    report
}

/// Constrói a config de um item de lote (o `body` é obrigatório).
fn config_from_params(parsed: RunParams, defaults: &Defaults) -> Result<RunConfig, Error> {
    let Some(goal) = parsed.body.clone() else {
        return Err(Error::invalid_input("item de lote sem `body`"));
    };
    Flags::default().assemble(goal, parsed, defaults)
}

#[cfg(test)]
mod tests;
