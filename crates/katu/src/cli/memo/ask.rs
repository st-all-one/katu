//! `memo ask`: consulta pelo caminho §42 (E20-T06/T08).
//!
//! Vive num módulo filho para manter `memo.rs` sob o teto. Aceita `--params` (XOR flags) e
//! `--batch` JSONL (valida tudo antes de executar).

use katu_core::error::Error;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cli::{input, params};
use crate::defaults;
use crate::memory::commands::memory_recall;
use crate::report::Report;

use super::AskArgs;

/// Parâmetros de uma consulta (JSON de `--params` ou de uma linha de `--batch`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AskParams {
    /// Consulta (alternativa ao posicional).
    query: Option<String>,
    /// Número máximo de resultados.
    limit: Option<usize>,
}

/// Executa `memo ask` (uma consulta ou um lote).
pub(super) fn report(args: &AskArgs) -> Report {
    if let Some(path) = &args.batch {
        return batch(args, path);
    }
    match resolve(args) {
        Ok((query, limit)) => memory_recall(&query, limit),
        Err(error) => Report::failed("memo.ask", &error),
    }
}

/// Resolve uma consulta a partir das flags e/ou de `--params` (XOR).
fn resolve(args: &AskArgs) -> Result<(String, usize), Error> {
    if args.params.is_some() && (args.query.is_some() || args.limit.is_some()) {
        return Err(Error::invalid_input(
            "--params é exclusivo com flags explícitas (não se infere)",
        ));
    }
    let parsed: AskParams = match &args.params {
        Some(raw) => params::parse(&params::source(raw)?)?,
        None => AskParams::default(),
    };
    if args.query.is_some() && parsed.query.is_some() {
        return Err(Error::invalid_input(
            "query posicional e `query` em --params são exclusivos",
        ));
    }
    let query = match &args.query {
        Some(query) => input::resolve(Some(query))?,
        None => match &parsed.query {
            Some(query) => query.clone(),
            None => input::resolve(None)?,
        },
    };
    let fallback = defaults::current().recall_limit.unwrap_or(5);
    Ok((query, args.limit.or(parsed.limit).unwrap_or(fallback)))
}

/// Processa um lote JSONL: valida tudo **antes** de executar.
fn batch(args: &AskArgs, path: &str) -> Report {
    if args.params.is_some() || args.query.is_some() || args.limit.is_some() {
        return Report::failed(
            "memo.ask",
            &Error::invalid_input("--batch é exclusivo com --params, query e limit"),
        );
    }
    let lines = match params::batch_lines(path) {
        Ok(lines) => lines,
        Err(error) => return Report::failed("memo.ask", &error),
    };
    let fallback = defaults::current().recall_limit.unwrap_or(5);
    let mut queries = Vec::with_capacity(lines.len());
    for line in &lines {
        match parse_line(line, fallback) {
            Ok(item) => queries.push(item),
            Err(error) => return Report::failed("memo.ask", &error),
        }
    }
    let mut items = Vec::with_capacity(queries.len());
    let mut exit = 0_u8;
    for (query, limit) in &queries {
        let report = memory_recall(query, *limit);
        if !report.success {
            exit = report.exit;
        }
        items.push(report.data.unwrap_or(Value::Null));
    }
    let mut report = Report::ok("memo.ask", Some(json!({ "items": items })));
    report.success = exit == 0;
    report.exit = exit;
    report
}

/// Lê uma linha de lote: uma string JSON (query) ou um objeto `{query, limit}`.
fn parse_line(line: &str, fallback: usize) -> Result<(String, usize), Error> {
    if line.starts_with('"') {
        let query: String = params::parse(line)?;
        return Ok((query, fallback));
    }
    let parsed: AskParams = params::parse(line)?;
    let Some(query) = parsed.query else {
        return Err(Error::invalid_input("item de lote sem `query`"));
    };
    Ok((query, parsed.limit.unwrap_or(fallback)))
}
