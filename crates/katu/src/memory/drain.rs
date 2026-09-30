//! Dreno da fila de embeddings (E20-T20): reconcilia o `.idx/` com o corpo canónico das notas.
//!
//! Reutiliza o pipeline do `knudge-core` (`embeddings::drain`) e o provedor configurado
//! (`http`/`lightweight`/`none`). **Fail-closed**: sem provedor ligado, não indexa nem falha —
//! devolve `enabled=false`; uma falha do provedor deixa as notas `pending` (nunca as descarta).

use std::path::PathBuf;

use katu_core::memory::MemoryError;
use knudge_core::adapters::HttpEmbedder;
use knudge_core::embeddings::{DrainInput, LightweightEmbedder, drain};
use knudge_core::ports::{Embedder, Env};
use knudge_core::{Config, Error as KnudgeError, Knudge};

use super::to_memory_error;

/// Teto de lotes por dreno (evita laço infinito num provedor que nunca esvazia).
const MAX_BATCHES: usize = 1024;

/// Resumo de um dreno de embeddings.
#[allow(
    clippy::struct_excessive_bools,
    reason = "`enabled` e `rebuilt` são estados independentes do dreno"
)]
pub(crate) struct DrainSummary {
    /// Se o provedor estava ligado (senão, foi no-op).
    pub(crate) enabled: bool,
    /// Lotes executados.
    pub(crate) batches: usize,
    /// Vetores indexados neste dreno.
    pub(crate) indexed: usize,
    /// Inferências evitadas pelo cache.
    pub(crate) cache_hits: usize,
    /// Se o `.idx/` foi apagado antes (modo `--force`).
    pub(crate) rebuilt: bool,
    /// Itens apagados em `--force`.
    pub(crate) removed: Vec<String>,
    /// Avisos de degradação graciosa.
    pub(crate) warnings: Vec<String>,
}

impl DrainSummary {
    /// Dreno desligado (sem provedor de embeddings).
    fn disabled() -> Self {
        Self {
            enabled: false,
            batches: 0,
            indexed: 0,
            cache_hits: 0,
            rebuilt: false,
            removed: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Drena a fila de embeddings do projeto aberto em `kd`.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`force` é o modo `--force` do dreno"
)]
pub(super) fn run(kd: &Knudge, force: bool) -> Result<DrainSummary, MemoryError> {
    let config = kd.config();
    let Some(embedder) = build_embedder(config, kd.env()).map_err(to_memory_error)? else {
        return Ok(DrainSummary::disabled());
    };
    let mut warnings = Vec::new();
    let mut removed = Vec::new();
    if force {
        removed = remove_derived(kd)?;
        warnings.push(format!(
            ".idx/ apagado ({} item(ns)); redigerindo tudo",
            removed.len()
        ));
    }
    let store = kd.store();
    let mut summary = DrainSummary {
        enabled: true,
        rebuilt: force,
        removed,
        ..DrainSummary::disabled()
    };
    loop {
        let outcome = drain(&DrainInput {
            store: &store,
            embedder: embedder.as_ref(),
            config,
            now_ms: kd.now_ms(),
        })
        .map_err(to_memory_error)?;
        warnings.extend(outcome.warnings);
        summary.batches = summary.batches.saturating_add(1);
        summary.indexed = summary.indexed.saturating_add(outcome.indexed);
        summary.cache_hits = summary.cache_hits.saturating_add(outcome.cache_hits);
        if outcome.indexed == 0 || summary.batches >= MAX_BATCHES {
            break;
        }
    }
    summary.warnings = warnings;
    Ok(summary)
}

/// Constrói o provedor de vetores a partir da config efetiva.
#[allow(
    clippy::type_complexity,
    reason = "provedor opcional (`None` = embeddings desligados)"
)]
fn build_embedder(
    config: &Config,
    env: &dyn Env,
) -> Result<Option<Box<dyn Embedder>>, KnudgeError> {
    if !config.get_bool("embeddings.enabled").unwrap_or(true) {
        return Ok(None);
    }
    match config.get_str("embeddings.provider").unwrap_or("http") {
        "http" => Ok(Some(Box::new(HttpEmbedder::new(config, env)?))),
        "lightweight" => {
            let dimensions = config
                .get_int("embeddings.dimensions")
                .and_then(|raw| usize::try_from(raw).ok())
                .unwrap_or(384);
            Ok(Some(Box::new(LightweightEmbedder::new(dimensions)?)))
        }
        "none" => Ok(None),
        other => Err(KnudgeError::config(format!(
            "embeddings.provider inválido: {other:?}"
        ))),
    }
}

/// Apaga `.idx/` (derivado) e devolve os nomes dos itens apagados.
fn remove_derived(kd: &Knudge) -> Result<Vec<String>, MemoryError> {
    let dir: PathBuf = kd.knowledge_dir().join(".idx");
    let fs = kd.fs_dyn();
    if !fs.exists(&dir) {
        return Ok(Vec::new());
    }
    let entries = fs.list_dir(&dir).map_err(to_memory_error)?;
    let names = entries
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    fs.remove_dir_all(&dir).map_err(to_memory_error)?;
    Ok(names)
}
