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
pub(super) fn clip(lines: &[&str], budget: ReadBudget) -> (String, bool) {
    let mut out = String::new();
    let mut bytes = 0_usize;
    for (index, line) in lines.iter().enumerate() {
        if index >= budget.max_lines {
            return (out, true);
        }
        let cost = line.len().saturating_add(1);
        if bytes.saturating_add(cost) > budget.max_bytes {
            return (out, true);
        }
        out.push_str(line);
        out.push('\n');
        bytes = bytes.saturating_add(cost);
    }
    (out, false)
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
