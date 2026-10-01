//! Tool `edit` **otimista** (E06-T03/OA16, Q-07): patch `old`→`new` com *compare-and-swap*.
//!
//! `read` → aplicar substituições → `Fs::write_atomic_if` (só grava se o conteúdo atual casar com o
//! lido). Se o ficheiro mudou entretanto, devolve `Unavailable { control: "stale" }` — **recuperável**
//! ("relê e reaplica"), nunca sobrescreve edição concorrente.
//!
//! **Multi-bloco atómico (Q-07).** Uma chamada aplica uma **lista** de substituições, por ordem, com
//! semântica *tudo ou nada*: se uma não casar exatamente uma vez, **nada é gravado** e o relatório
//! diz **qual** falhou e **que âncoras únicas existem perto** (Q-08) — o modelo não fica a adivinhar
//! nem deixa o ficheiro a meio de um refactor. `dry_run` mostra o patch sem gravar.
//!
//! A atomicidade não é só conveniência: com N chamadas separadas, a que falha a meio deixa o
//! ficheiro refatorizado pela metade (medido em [`bench`]), e o custo em *payload* de chamada
//! repete o `path` e o envelope N vezes.

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Fs, FsError};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ResolvedPath, ToolArgs, ToolName, ToolUse};

use crate::diff::{Diff, unified};
use crate::lang::to_i64;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;

/// Trechos mínimos para uma âncora candidata ser útil (abaixo disto é ruído).
const MIN_ANCHOR_CHARS: usize = 3;

/// Teto de âncoras devolvidas numa recusa (bytes do caminho de erro são do modelo, DF12).
pub const MAX_ANCHORS: usize = 3;

/// Uma substituição: `old` tem de ser **único** no texto no momento em que é aplicada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// Trecho a substituir.
    pub old: String,
    /// Trecho que o substitui.
    pub new: String,
}

impl Replacement {
    /// Constrói uma substituição.
    #[must_use]
    pub fn new(old: impl Into<String>, new: impl Into<String>) -> Self {
        let _span = katu_core::trace_fn!("edit::replacement_new");

        Self {
            old: old.into(),
            new: new.into(),
        }
    }
}

/// Porque é que uma substituição não foi aplicada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// O trecho não existe no texto (nem uma vez).
    NotFound,
    /// O trecho existe mais do que uma vez (a substituição seria ambígua).
    Ambiguous,
}

impl FailureKind {
    /// Nome estável (aparece no relatório e no TOON).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::Ambiguous => "ambiguous",
        }
    }

    /// O que o modelo deve fazer a seguir (Q-08: a recusa ensina).
    #[must_use]
    pub const fn remedy(self) -> &'static str {
        match self {
            Self::NotFound => {
                "usa uma das âncoras abaixo como `old` (ou relê o ficheiro); nada foi gravado"
            }
            Self::Ambiguous => {
                "acrescenta contexto à âncora até ela aparecer uma só vez (ou usa `range` no `read`); \
                 nada foi gravado"
            }
        }
    }
}

/// Falha de aplicação: **qual** substituição, **porquê** e **o que existe perto**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditFailure {
    /// Índice da substituição que falhou (base 0, na ordem da chamada).
    pub index: usize,
    /// Motivo.
    pub kind: FailureKind,
    /// Âncoras únicas mais próximas (`"linha: texto"`), no máximo [`MAX_ANCHORS`].
    pub anchors: Vec<String>,
}

/// Aplica a lista de substituições **atomicamente**: ou devolve o texto final, ou a falha.
///
/// Determinística (mesma entrada → mesma saída), por ordem da lista, e sem tocar no original: o
/// resultado é construído numa cópia, pelo que uma falha a meio não deixa efeitos.
pub fn apply(text: &str, replacements: &[Replacement]) -> Result<String, EditFailure> {
    let _span = katu_core::trace_fn!("edit::apply");

    let mut out = text.to_string();
    for (index, replacement) in replacements.iter().enumerate() {
        match out.matches(&replacement.old).count() {
            0 => {
                return Err(EditFailure {
                    index,
                    kind: FailureKind::NotFound,
                    anchors: nearest_anchors(&out, &replacement.old),
                });
            }
            1 => out = out.replacen(&replacement.old, &replacement.new, 1),
            count => {
                return Err(EditFailure {
                    index,
                    kind: FailureKind::Ambiguous,
                    anchors: match_lines(&out, &replacement.old, count),
                });
            }
        }
    }
    Ok(out)
}

/// Linhas em que o trecho casa (para a recusa por ambiguidade): `"linha: texto"`.
fn match_lines(text: &str, old: &str, count: usize) -> Vec<String> {
    let _span = katu_core::trace_fn!("edit::match_lines");

    let mut out = Vec::with_capacity(count.min(MAX_ANCHORS));
    for (number, line) in text.lines().enumerate() {
        if out.len() >= MAX_ANCHORS {
            break;
        }
        if line.contains(old) || old.contains(line) {
            out.push(describe(number, line));
        }
    }
    out
}

/// Âncoras únicas mais próximas do trecho procurado (Q-08): as linhas que **não** se repetem e
/// partilham o maior prefixo com a primeira linha de `old`.
///
/// Determinística: ordena por `(prefixo comum desc, número da linha asc)` e corta em [`MAX_ANCHORS`].
/// Só a primeira linha do trecho é usada como sonda — é o que o modelo costuma acertar.
fn nearest_anchors(text: &str, old: &str) -> Vec<String> {
    let _span = katu_core::trace_fn!("edit::nearest_anchors");

    let probe = old
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(old);
    let probe = probe.trim();
    let mut ranked: Vec<(usize, usize, String)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.len() < MIN_ANCHOR_CHARS || !is_unique(text, trimmed) {
            continue;
        }
        let shared = common_prefix_chars(trimmed, probe);
        if shared < MIN_ANCHOR_CHARS {
            continue;
        }
        ranked.push((shared, number, line.to_string()));
    }
    ranked.sort_by(|left, right| right.cmp(left));
    ranked.truncate(MAX_ANCHORS);
    ranked
        .into_iter()
        .map(|(_, number, line)| describe(number, &line))
        .collect()
}

/// `true` se a linha aparece uma só vez no texto (uma âncora única).
fn is_unique(text: &str, line: &str) -> bool {
    let _span = katu_core::trace_fn!("edit::is_unique");

    text.matches(line).count() == 1
}

/// Caracteres iniciais comuns (a comparação para é na primeira diferença).
fn common_prefix_chars(left: &str, right: &str) -> usize {
    let _span = katu_core::trace_fn!("edit::common_prefix_chars");

    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

/// `"12: let x = 1;"` (número da linha a partir de 1, como o modelo vê no `read`).
fn describe(number: usize, line: &str) -> String {
    let _span = katu_core::trace_fn!("edit::describe");

    format!("{}: {}", number.saturating_add(1), line.trim())
}

/// Executor de um patch otimista (uma ou mais substituições, atómicas).
pub struct EditFileTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Substituições aplicadas **por ordem** e **atomicamente**.
    pub replacements: Vec<Replacement>,
    /// Se `true`, não grava (mostra o patch).
    pub dry_run: bool,
}

impl Tool for EditFileTool<'_> {
    fn name(&self) -> ToolName {
        let _span = katu_core::trace_fn!("edit::name");

        ToolName::Edit
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_EDIT, "edit::execute");
        let ToolArgs::Edit { path } = &use_.args else {
            return unavailable("edit");
        };
        let Ok(current) = self.fs.read(Path::new(path.as_str())) else {
            return unavailable("read");
        };
        let text = String::from_utf8_lossy(&current);
        let updated = match apply(&text, &self.replacements) {
            Ok(updated) => updated,
            Err(failure) => return rejected(path, &failure, self.replacements.len()),
        };
        let id = content_id("f", path.as_str().as_bytes());
        let old_hash = content_hash(&current);
        let new_hash = content_hash(updated.as_bytes());
        let delta = unified(text.as_ref(), updated.as_str(), 3);
        let patch = Patch {
            path,
            id: &id,
            old_hash: &old_hash,
            new_hash: &new_hash,
            delta: &delta,
            edits: self.replacements.len(),
        };
        if self.dry_run {
            return ToolOutput::report(report("edit.dry-run", &patch));
        }
        match self
            .fs
            .write_atomic_if(Path::new(path.as_str()), updated.as_bytes(), &current)
        {
            Ok(()) => ToolOutput::report(report("edit.patch", &patch)),
            Err(FsError::Stale) => unavailable("stale"),
            Err(_) => unavailable("write"),
        }
    }
}

/// Campos do relatório de um patch (agrupa os argumentos; `report` fica com um só).
#[derive(Clone, Copy)]
struct Patch<'a> {
    path: &'a ResolvedPath,
    id: &'a str,
    old_hash: &'a str,
    new_hash: &'a str,
    delta: &'a Diff,
    edits: usize,
}

fn report(kind: &'static str, patch: &Patch<'_>) -> ToolReport {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_EDIT, "edit::report");
    let Patch {
        path,
        id,
        old_hash,
        new_hash,
        delta,
        edits,
    } = *patch;
    let data = Value::map(vec![
        ("path".to_string(), Value::str(path.as_str())),
        (
            "edits".to_string(),
            Value::int(to_i64(u64::try_from(edits).unwrap_or(u64::MAX))),
        ),
        (
            "hunks".to_string(),
            Value::int(to_i64(u64::try_from(delta.hunks.len()).unwrap_or(u64::MAX))),
        ),
        ("added".to_string(), Value::int(i64::from(delta.added))),
        ("removed".to_string(), Value::int(i64::from(delta.removed))),
        ("old_hash".to_string(), Value::str(old_hash)),
        ("new_hash".to_string(), Value::str(new_hash)),
        ("breaking".to_string(), Value::bool(false)),
    ]);
    ToolReport::new(kind, data).with_id(id).with_hash(new_hash)
}

/// Relatório de uma recusa que **ensina** (Q-08): qual substituição, porquê, âncoras e remédio.
///
/// O efeito continua a ser `Unavailable` (recuperável) e o `report` viaja no delta (§18/G6), pelo
/// que o modelo vê exatamente o que existe perto do sítio onde falhou.
fn rejected(path: &ResolvedPath, failure: &EditFailure, total: usize) -> ToolOutput {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_EDIT, "edit::rejected");
    let data = Value::map(vec![
        ("path".to_string(), Value::str(path.as_str())),
        (
            "edit".to_string(),
            Value::int(to_i64(u64::try_from(failure.index).unwrap_or(u64::MAX))),
        ),
        (
            "edits".to_string(),
            Value::int(to_i64(u64::try_from(total).unwrap_or(u64::MAX))),
        ),
        ("reason".to_string(), Value::str(failure.kind.as_str())),
        (
            "anchors".to_string(),
            Value::list(
                failure
                    .anchors
                    .iter()
                    .map(|anchor| Value::str(anchor.as_str()))
                    .collect(),
            ),
        ),
        ("remedy".to_string(), Value::str(failure.kind.remedy())),
    ]);
    // Sem `next`: as âncoras já viajam em `anchors` (duplicá-las só gastaria bytes do modelo).
    let report =
        ToolReport::new("edit.rejected", data).with_id(content_id("f", path.as_str().as_bytes()));
    ToolOutput {
        outcome: ToolOutcome::Unavailable {
            control: ControlId::new(failure.kind.as_str()),
            rule_id: None,
        },
        report: Some(report),
    }
}

fn unavailable(control: &'static str) -> ToolOutput {
    let _span = katu_core::trace_fn!("edit::unavailable");

    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}
