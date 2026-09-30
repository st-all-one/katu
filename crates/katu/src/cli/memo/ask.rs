//! `memo ask`/`memo knowledge`: consulta rica pelo adaptador (E20-T06).
//!
//! Espelha `kd ask`/`kd map` (filtros, modos `--rank`/`--tags`/`--suggest`, `--id`/`--around`).
//! Aceita `--params` (XOR flags) e `--batch` JSONL (valida tudo antes de executar).

mod input;

use katu_core::error::Error;
use katu_core::memory::{Anchor, QueryFilter, QueryMode, QueryReq, QueryUniverse};
use serde_json::Value;

use crate::cli::params;
use crate::defaults;
use crate::memory::commands::memory_query;
use crate::report::Report;

use super::args::{AskArgs, KnowledgeArgs};
use input::{AskParams, Input, parse_types};

/// Executa `memo ask` (uma consulta ou um lote).
pub(super) fn report(args: &AskArgs) -> Report {
    if let Some(path) = &args.batch {
        return batch(args, path);
    }
    match resolve(args) {
        Ok(req) => memory_query("memo.ask", &req),
        Err(error) => Report::failed("memo.ask", &error),
    }
}

/// Executa `memo knowledge` (mapa estrutural).
pub(super) fn knowledge(args: &KnowledgeArgs) -> Report {
    let fallback = defaults::current().recall_limit.unwrap_or(5);
    match knowledge_query(args, fallback) {
        Ok(req) => memory_query("memo.knowledge", &req),
        Err(error) => Report::failed("memo.knowledge", &error),
    }
}

/// Constrói o `QueryReq` do mapa.
fn knowledge_query(args: &KnowledgeArgs, default_limit: usize) -> Result<QueryReq, Error> {
    Ok(QueryReq {
        mode: QueryMode::Map,
        filter: QueryFilter {
            types: parse_types(&args.types)?,
            classes: args.classes.clone(),
            statuses: Vec::new(),
            tags: args.tags.clone(),
            anchors: args
                .anchor
                .iter()
                .map(|path| Anchor::new(path.clone()))
                .collect(),
            scope: args.scope.clone(),
        },
        around: args.around.clone(),
        depth: args.depth,
        axis: args.axis.clone(),
        members: args.members,
        universe: if args.universe {
            QueryUniverse::All
        } else {
            QueryUniverse::Knowledge
        },
        limit: args.limit.unwrap_or(default_limit),
        ..QueryReq::default()
    })
}

/// Resolve uma consulta a partir das flags e/ou de `--params` (XOR).
fn resolve(args: &AskArgs) -> Result<QueryReq, Error> {
    if args.params.is_some() && flags_used(args) {
        return Err(Error::invalid_input(
            "--params é exclusivo com flags explícitas (não se infere)",
        ));
    }
    let fallback = defaults::current().recall_limit.unwrap_or(5);
    let input = match &args.params {
        Some(raw) => Input::from_params(params::parse(&params::source(raw)?)?),
        None => Input::from_flags(args),
    };
    input.to_query(fallback)
}

/// `true` se alguma flag explícita (fora do posicional) foi usada.
fn flags_used(args: &AskArgs) -> bool {
    !args.ids.is_empty()
        || args.around.is_some()
        || args.via.is_some()
        || args.depth != 1
        || args.brief
        || args.with_task
        || args.full_content
        || !args.types.is_empty()
        || !args.classes.is_empty()
        || !args.tags.is_empty()
        || args.status.is_some()
        || args.scope.is_some()
        || !args.anchor.is_empty()
        || args.since.is_some()
        || args.until.is_some()
        || args.as_of.is_some()
        || args.limit.is_some()
        || args.rank
        || args.tags_vocab
        || args.suggest
        || args.universe
        || args.top_k != 5
        || args.relation.is_some()
}

/// Processa um lote JSONL: valida tudo **antes** de executar.
fn batch(args: &AskArgs, path: &str) -> Report {
    if args.params.is_some() || flags_used(args) || args.query.is_some() {
        return Report::failed(
            "memo.ask",
            &Error::invalid_input("--batch é exclusivo com --params, flags e query"),
        );
    }
    let lines = match params::batch_lines(path) {
        Ok(lines) => lines,
        Err(error) => return Report::failed("memo.ask", &error),
    };
    let fallback = defaults::current().recall_limit.unwrap_or(5);
    let mut queries = Vec::with_capacity(lines.len());
    for line in &lines {
        let parsed: AskParams = match params::parse(line) {
            Ok(parsed) => parsed,
            Err(error) => return Report::failed("memo.ask", &error),
        };
        match Input::from_params(parsed).to_query(fallback) {
            Ok(req) => queries.push(req),
            Err(error) => return Report::failed("memo.ask", &error),
        }
    }
    let mut items = Vec::with_capacity(queries.len());
    let mut exit = 0_u8;
    for req in &queries {
        let report = memory_query("memo.ask", req);
        if !report.success {
            exit = report.exit;
        }
        items.push(report.data.unwrap_or(Value::Null));
    }
    let mut report = Report::ok("memo.ask", Some(serde_json::json!({ "items": items })));
    report.success = exit == 0;
    report.exit = exit;
    report
}
