//! Helpers puros das views (linhas, orçamento, imports, flags, linguagem).

use crate::read::ReadBudget;

pub(super) use crate::lang::{language, len_u64, to_i64};

/// Linhas `[start, end]` (1-based), já clampeadas.
pub(super) fn slice<'a>(lines: &[&'a str], start: u32, end: u32) -> Vec<&'a str> {
    let skip = usize::try_from(start.saturating_sub(1)).unwrap_or(0);
    let take = usize::try_from(end.saturating_sub(start).saturating_add(1)).unwrap_or(0);
    lines.iter().skip(skip).take(take).copied().collect()
}

/// Junta linhas até ao orçamento; devolve `(texto, truncado)`.
///
/// A truncagem é determinística e **nunca** parte um carácter UTF-8: se a primeira linha elegível
/// não couber (um "chunk único enorme"), devolve um prefixo cortado num limite de carácter —
/// garantir progresso na paginação em vez de devolver vazio.
pub(super) fn clip(lines: &[&str], budget: ReadBudget) -> (String, bool) {
    let mut out = String::new();
    let mut bytes = 0_usize;
    for (index, line) in lines.iter().enumerate() {
        if index >= budget.max_lines {
            return (out, true);
        }
        let cost = line.len().saturating_add(1);
        if bytes.saturating_add(cost) > budget.max_bytes {
            if out.is_empty() {
                // Linha única maior que o orçamento: corta num limite de carácter (progresso).
                let room = budget.max_bytes.saturating_sub(1);
                let prefix = char_boundary(line, room);
                if !prefix.is_empty() {
                    out.push_str(prefix);
                    out.push('\n');
                }
            }
            return (out, true);
        }
        out.push_str(line);
        out.push('\n');
        bytes = bytes.saturating_add(cost);
    }
    (out, false)
}

/// Prefixo de `line` com no máximo `max_bytes` bytes, cortado num limite de carácter UTF-8.
fn char_boundary(line: &str, max_bytes: usize) -> &str {
    if line.len() <= max_bytes {
        return line;
    }
    let mut end = max_bytes.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    line.get(..end).unwrap_or("")
}

pub(super) fn imports(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            line.starts_with("use ")
                || line.starts_with("import ")
                || line.starts_with("from ")
                || line.starts_with("#include")
        })
        .map(|line| line.trim_end_matches(';').to_string())
        .take(32)
        .collect()
}

pub(super) fn flags(lines: &[&str]) -> Vec<(u32, &'static str)> {
    const KINDS: &[&str] = &["TODO", "FIXME", "XXX", "HACK"];
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(kind) = KINDS.iter().find(|kind| line.contains(**kind)) {
            found.push((
                u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
                *kind,
            ));
        }
    }
    found.truncate(16);
    found
}
