//! Diff unificado determinístico (E06-T03): sem dependências, sem LLM.
//!
//! Compara duas versões por linhas com prefixo/sufixo comum (O(n)); o miolo é removido/adicionado.
//! Para "só o delta" (G6) basta e evita a matriz O(n·m) de um LCS completo.

/// Linha de um diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// Linha igual nos dois lados.
    Context(String),
    /// Linha removida.
    Remove(String),
    /// Linha adicionada.
    Add(String),
}

/// Bloco de alterações (contíguo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Primeira linha no lado antigo (1-based).
    pub old_start: u32,
    /// Número de linhas no lado antigo.
    pub old_len: u32,
    /// Primeira linha no lado novo (1-based).
    pub new_start: u32,
    /// Número de linhas no lado novo.
    pub new_len: u32,
    /// Linhas do bloco (`Context`/`Remove`/`Add`).
    pub lines: Vec<DiffLine>,
}

/// Resultado de um diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// Blocos de alterações (vazio = sem alterações).
    pub hunks: Vec<Hunk>,
    /// Linhas adicionadas.
    pub added: u32,
    /// Linhas removidas.
    pub removed: u32,
}

/// Diff unificado com `context` linhas de contexto à volta do miolo alterado.
#[must_use]
pub fn unified(old: &str, new: &str, context: usize) -> Diff {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = common_prefix(&a, &b);
    let suffix = common_suffix(&a, &b, prefix);
    let removed = a.len().saturating_sub(prefix).saturating_sub(suffix);
    let added = b.len().saturating_sub(prefix).saturating_sub(suffix);
    if removed == 0 && added == 0 {
        return Diff::default();
    }
    Diff {
        hunks: vec![build_hunk(
            &a,
            &b,
            Change {
                prefix,
                removed,
                added,
            },
            context,
        )],
        added: to_u32(added),
        removed: to_u32(removed),
    }
}

/// Miolo alterado: prefixo comum + linhas removidas/adicionadas.
#[derive(Clone, Copy)]
struct Change {
    /// Linhas iguais no início.
    prefix: usize,
    /// Linhas removidas.
    removed: usize,
    /// Linhas adicionadas.
    added: usize,
}

/// Comprimento do prefixo comum de linhas.
fn common_prefix(a: &[&str], b: &[&str]) -> usize {
    let mut index = 0;
    while index < a.len() && index < b.len() && a.get(index) == b.get(index) {
        index = index.saturating_add(1);
    }
    index
}

/// Comprimento do sufixo comum de linhas (sem sobrepor o prefixo).
fn common_suffix(a: &[&str], b: &[&str], prefix: usize) -> usize {
    let mut count = 0;
    while count < a.len().saturating_sub(prefix)
        && count < b.len().saturating_sub(prefix)
        && a.get(a.len().saturating_sub(1).saturating_sub(count))
            == b.get(b.len().saturating_sub(1).saturating_sub(count))
    {
        count = count.saturating_add(1);
    }
    count
}

/// Constrói o único bloco: contexto antes + removidas + adicionadas + contexto depois.
fn build_hunk(a: &[&str], b: &[&str], change: Change, context: usize) -> Hunk {
    let Change {
        prefix,
        removed,
        added,
    } = change;
    let ctx_start = prefix.saturating_sub(context);
    let after = prefix.saturating_add(removed);
    let ctx_end = after.saturating_add(context).min(a.len());
    let mut lines = Vec::new();
    for line in a.iter().take(prefix).skip(ctx_start) {
        lines.push(DiffLine::Context((*line).to_string()));
    }
    for line in a.iter().skip(prefix).take(removed) {
        lines.push(DiffLine::Remove((*line).to_string()));
    }
    for line in b.iter().skip(prefix).take(added) {
        lines.push(DiffLine::Add((*line).to_string()));
    }
    for line in a.iter().skip(after).take(ctx_end.saturating_sub(after)) {
        lines.push(DiffLine::Context((*line).to_string()));
    }
    let old_len = ctx_end.saturating_sub(ctx_start);
    let new_len = old_len.saturating_sub(removed).saturating_add(added);
    Hunk {
        old_start: to_u32(ctx_start.saturating_add(1)),
        old_len: to_u32(old_len),
        new_start: to_u32(ctx_start.saturating_add(1)),
        new_len: to_u32(new_len),
        lines,
    }
}

/// Converte um comprimento para `u32` (satura; nunca falha).
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{DiffLine, unified};

    #[test]
    fn identical_texts_produce_no_hunks() {
        let diff = unified("a\nb\n", "a\nb\n", 3);
        assert!(diff.hunks.is_empty());
        assert_eq!(diff.added, 0);
        assert_eq!(diff.removed, 0);
    }

    #[test]
    fn single_line_change_keeps_context() -> Result<(), Box<dyn std::error::Error>> {
        let diff = unified("a\nb\nc\nd\n", "a\nB\nc\nd\n", 1);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 1);
        let hunk = diff.hunks.first().ok_or("sem hunk")?;
        assert_eq!(hunk.old_start, 1);
        assert_eq!(
            hunk.lines.first(),
            Some(&DiffLine::Context("a".to_string()))
        );
        assert!(hunk.lines.contains(&DiffLine::Remove("b".to_string())));
        assert!(hunk.lines.contains(&DiffLine::Add("B".to_string())));
        assert!(hunk.lines.contains(&DiffLine::Context("c".to_string())));
        Ok(())
    }

    #[test]
    fn pure_addition_at_the_end() -> Result<(), Box<dyn std::error::Error>> {
        let diff = unified("a\nb\n", "a\nb\nc\n", 3);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 0);
        let hunk = diff.hunks.first().ok_or("sem hunk")?;
        assert!(hunk.lines.contains(&DiffLine::Add("c".to_string())));
        assert_eq!(hunk.new_len, hunk.old_len.saturating_add(1));
        Ok(())
    }

    #[test]
    fn pure_removal_at_the_start() -> Result<(), Box<dyn std::error::Error>> {
        let diff = unified("a\nb\nc\n", "b\nc\n", 3);
        assert_eq!(diff.added, 0);
        assert_eq!(diff.removed, 1);
        let hunk = diff.hunks.first().ok_or("sem hunk")?;
        assert!(hunk.lines.contains(&DiffLine::Remove("a".to_string())));
        Ok(())
    }
}
