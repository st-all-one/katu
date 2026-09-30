//! Outline heurístico (E06-T03): símbolos de um ficheiro de código, **sem tree-sitter**.
//!
//! Heurística leve e determinística, Rust-first: reconhece declarações por prefixo de linha e
//! calcula o fim por contagem de chaves. **Não** é um parser (não ignora `{}` dentro de strings ou
//! comentários) — é o suficiente para o modelo decidir onde ir; o tree-sitter fica gated por
//! medição (DF12). Ordem canónica: a ordem do ficheiro.

use katu_core::diag::{Level, events};

/// Tipo de símbolo reconhecido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Função.
    Fn,
    /// Estrutura.
    Struct,
    /// Enumeração.
    Enum,
    /// Traço.
    Trait,
    /// Implementação.
    Impl,
    /// Módulo.
    Mod,
    /// Constante.
    Const,
    /// Estático.
    Static,
    /// Alias de tipo.
    Type,
    /// Macro.
    Macro,
    /// Importação.
    Use,
}

impl SymbolKind {
    /// Nome estável (`snake_case`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Mod => "mod",
            Self::Const => "const",
            Self::Static => "static",
            Self::Type => "type",
            Self::Macro => "macro",
            Self::Use => "use",
        }
    }
}

/// Símbolo com nome e range de linhas (1-based, inclusivo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Tipo.
    pub kind: SymbolKind,
    /// Nome (ou alvo de `impl`).
    pub name: String,
    /// Linha inicial (1-based).
    pub start: u32,
    /// Linha final (1-based, inclusiva).
    pub end: u32,
}

impl Symbol {
    /// Número de linhas.
    #[must_use]
    pub const fn lines(&self) -> u32 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }
}

/// Extrai os símbolos de um texto (ordem do ficheiro).
#[must_use]
pub fn outline(text: &str) -> Vec<Symbol> {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_READ, "outline::outline");
    let lines: Vec<&str> = text.lines().collect();
    let mut symbols = Vec::new();
    let mut index = 0_usize;
    while let Some(line) = lines.get(index) {
        if let Some((kind, name)) = classify(line) {
            let end = block_end(&lines, index);
            symbols.push(Symbol {
                kind,
                name,
                start: to_line(index),
                end: to_line(end),
            });
        }
        // Avança 1: símbolos **aninhados** (métodos dentro de `impl`) também contam.
        index = index.saturating_add(1);
    }
    symbols
}

fn to_line(index: usize) -> u32 {
    let _span = katu_core::trace_fn!("outline::to_line");

    u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1)
}

/// Classifica uma linha como declaração, devolvendo o tipo e o nome.
fn classify(line: &str) -> Option<(SymbolKind, String)> {
    let _span = katu_core::trace_fn!("outline::classify");

    let normalized = normalize(line);
    let (kind, rest) = keyword(&normalized)?;
    let name = if kind == SymbolKind::Impl {
        rest.split('{').next().unwrap_or(rest).trim().to_string()
    } else {
        take_ident(rest)?
    };
    if name.is_empty() {
        None
    } else {
        Some((kind, name))
    }
}

/// Remove `pub`/`pub(...)` e modificadores (`async`/`unsafe`/`default`); `const fn` vira `fn`.
fn normalize(line: &str) -> String {
    let _span = katu_core::trace_fn!("outline::normalize");

    let mut rest = strip_visibility(line.trim_start());
    loop {
        let stripped = rest
            .strip_prefix("async ")
            .or_else(|| rest.strip_prefix("unsafe "))
            .or_else(|| rest.strip_prefix("default "));
        match stripped {
            Some(inner) => rest = inner,
            None => break,
        }
    }
    match rest.strip_prefix("const fn ") {
        Some(inner) => format!("fn {inner}"),
        None => rest.to_string(),
    }
}

fn strip_visibility(line: &str) -> &str {
    let _span = katu_core::trace_fn!("outline::strip_visibility");

    if let Some(rest) = line.strip_prefix("pub(")
        && let Some(close) = rest.find(')')
    {
        return rest
            .get(close.saturating_add(1)..)
            .unwrap_or("")
            .trim_start();
    }
    line.strip_prefix("pub ").unwrap_or(line)
}

fn keyword(trimmed: &str) -> Option<(SymbolKind, &str)> {
    const PREFIXES: &[(&str, SymbolKind)] = &[
        ("fn ", SymbolKind::Fn),
        ("struct ", SymbolKind::Struct),
        ("enum ", SymbolKind::Enum),
        ("trait ", SymbolKind::Trait),
        ("mod ", SymbolKind::Mod),
        ("const ", SymbolKind::Const),
        ("static ", SymbolKind::Static),
        ("type ", SymbolKind::Type),
        ("macro_rules! ", SymbolKind::Macro),
        ("use ", SymbolKind::Use),
    ];
    for (prefix, kind) in PREFIXES {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((*kind, rest));
        }
    }
    trimmed
        .strip_prefix("impl")
        .map(|rest| (SymbolKind::Impl, rest.trim_start()))
}

fn take_ident(rest: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("outline::take_ident");

    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Fim do bloco: conta chaves a partir da linha; sem `{` na primeira linha, é a própria linha.
fn block_end(lines: &[&str], start: usize) -> usize {
    let _span = katu_core::trace_fn!("outline::block_end");

    let first = lines.get(start).copied().unwrap_or("");
    if !first.contains('{') {
        return start;
    }
    let mut depth: i32 = 0;
    let mut opened = false;
    let mut index = start;
    while let Some(line) = lines.get(index) {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth = depth.saturating_add(1);
                    opened = true;
                }
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if opened && depth <= 0 {
            return index;
        }
        index = index.saturating_add(1);
    }
    lines.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::{SymbolKind, outline};

    #[test]
    fn extracts_rust_symbols_in_order() {
        let text = "\
use std::fmt;

pub struct Point {
    x: i32,
}

impl Point {
    pub fn new(x: i32) -> Self {
        Self { x }
    }
}

fn helper() {}

mod inner;
";
        let symbols = outline(text);
        let names: Vec<(&str, &str)> = symbols
            .iter()
            .map(|s| (s.kind.as_str(), s.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("use", "std"),
                ("struct", "Point"),
                ("impl", "Point"),
                ("fn", "new"),
                ("fn", "helper"),
                ("mod", "inner"),
            ]
        );
        let point = symbols.get(1);
        assert_eq!(point.map(|s| s.start), Some(3));
        assert_eq!(point.map(|s| s.end), Some(5));
    }

    #[test]
    fn handles_modifiers_and_visibility() {
        let text = "\
pub(crate) async fn fetch() {}
unsafe fn raw() {}
const fn constant() {}
";
        let symbols = outline(text);
        assert_eq!(symbols.len(), 3);
        assert!(symbols.iter().all(|s| s.kind == SymbolKind::Fn));
    }

    #[test]
    fn use_line_ends_on_itself() {
        let symbols = outline("use a::b;\nfn x() {}\n");
        assert_eq!(symbols.first().map(|s| s.end), Some(1));
    }
}
