//! Codificação/parse dos artefactos de auditoria: manifesto e tabela `a` (segmento colunar).
//!
//! A leitura é uma desserialização mínima do formato colunar (linhas `\x1f` de uma secção `\x1e`),
//! segura porque as células são sanitizadas (sem `\n`/`\x1f`). O índice binário vive em [`super::bin`].

use std::path::Path;

use super::record::AuditRecord;
use super::store::{AUDIT_SCHEMA_VERSION, AuditError, Manifest};
use crate::ports::Fs;

/// Prefixo de secção (Record Separator).
const RS: char = '\u{1e}';
/// Separador de células (Unit Separator).
const US: char = '\u{1f}';

/// Lê o manifesto (default se ausente; erro se a versão não for suportada).
pub(super) fn read_manifest(fs: &dyn Fs, dir: &Path) -> Result<Manifest, AuditError> {
    let path = dir.join("manifest.json");
    if !fs.exists(&path) {
        return Ok(Manifest::default());
    }
    let bytes = fs.read(&path)?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|err| AuditError::Manifest(err.to_string()))?;
    if manifest.schema_version != AUDIT_SCHEMA_VERSION {
        return Err(AuditError::Manifest(format!(
            "versão {} não suportada",
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

/// Grava o manifesto atomicamente.
pub(super) fn write_manifest(
    fs: &dyn Fs,
    dir: &Path,
    manifest: &Manifest,
) -> Result<(), AuditError> {
    let bytes =
        serde_json::to_vec(manifest).map_err(|err| AuditError::Manifest(err.to_string()))?;
    fs.write_atomic(&dir.join("manifest.json"), &bytes)?;
    Ok(())
}

/// Reconstrói uma linha de auditoria a partir da tabela `a`.
pub(super) fn parse_record(row: &[String]) -> Result<AuditRecord, AuditError> {
    let get = |index: usize| row.get(index).cloned().unwrap_or_default();
    let seq = get(0)
        .parse::<u64>()
        .map_err(|err| AuditError::Parse(format!("seq inválido: {err}")))?;
    Ok(AuditRecord {
        seq,
        kind: kind_of(&get(1)),
        tool: get(2),
        path: get(3),
        status: get(4),
        rule: get(5),
        text: get(6),
    })
}

/// Mapeia o `kind` textual de volta ao domínio fechado.
fn kind_of(text: &str) -> &'static str {
    match text {
        "turn" => "turn",
        "user" => "user",
        "assistant" => "assistant",
        "tool_call" => "tool_call",
        "tool_result" => "tool_result",
        "phase" => "phase",
        "waiver" => "waiver",
        "plan" => "plan",
        "command" => "command",
        "workspace" => "workspace",
        "approval" => "approval",
        "verify" => "verify",
        "control" => "control",
        _ => "event",
    }
}

/// Lê um ficheiro como texto.
pub(super) fn read_text(fs: &dyn Fs, path: &Path) -> Result<String, AuditError> {
    let bytes = fs.read(path)?;
    String::from_utf8(bytes).map_err(|err| AuditError::Parse(err.to_string()))
}

/// Lê as linhas de uma secção de tabela (`\x1eNOME\n` + linhas `\x1f` até à secção seguinte).
pub(super) fn parse_rows(text: &str, name: &str) -> Vec<Vec<String>> {
    let marker = format!("{RS}{name}\n");
    let Some(start) = text.find(&marker) else {
        return Vec::new();
    };
    let body = text.get(start.saturating_add(marker.len())..).unwrap_or("");
    let mut rows = Vec::new();
    for line in body.split('\n') {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(RS) || line.starts_with('\u{1d}') {
            break;
        }
        rows.push(line.split(US).map(str::to_string).collect());
    }
    rows
}
