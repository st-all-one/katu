//! Linter de schema de tools (E06-T02): nomes canónicos, descrições que ensinam e defesa contra
//! *prompt poisoning*.
//!
//! O linter reusa [`katu_core::validate::Issue`] para devolver os problemas **agregados** (nunca
//! uma `String` solta): cada `Issue` aponta o campo exato e ensina a corrigir. As especificações
//! reais vivem em [`specs`]; o gate do CI (`xtask check-schemas`) corre [`lint_all`].

mod specs;

#[cfg(test)]
mod tests;

pub use specs::SCHEMAS;

use std::collections::BTreeSet;

use katu_core::validate::{Issue, Issues};

/// Conjunto fechado de verbos aceites como primeira palavra de um nome de tool (ordem
/// verbo-substantivo; `read`/`write`/`edit`/… são o verbo, o resto é o substantivo).
const VERBS: &[&str] = &[
    "read", "write", "edit", "move", "trash", "find", "grep", "ls", "exec", "bash", "search",
    "plan", "memory", "record", "recall", "close", "outcome", "compact", "model", "thinking",
];

/// Marcadores de *poisoning* proibidos em nomes e descrições (anti-injeção de prompt).
const POISON: &[&str] = &[
    "<system>",
    "<assistant>",
    "</",
    "<!--",
    "```",
    "ignore previous",
    "ignore all previous",
    "disregard",
];

/// Teto da descrição de uma tool (caracteres).
pub const MAX_DESCRIPTION_CHARS: usize = 1024;

/// Tipo de um parâmetro (conjuntos fechados usam [`ParamKind::Enum`]; ids usam [`ParamKind::Id`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind<'a> {
    /// Texto livre.
    Text,
    /// Inteiro.
    Integer,
    /// Booleano.
    Boolean,
    /// Lista de textos.
    ListText,
    /// Caminho (resolvido antes da política).
    Path,
    /// Conjunto **fechado** de valores (nunca uma `String` livre).
    Enum(&'a [&'a str]),
    /// Identificador tipado com um `pattern` (obrigatório e não vazio).
    Id(&'a str),
}

/// Especificação de um parâmetro.
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec<'a> {
    /// Nome `snake_case`.
    pub name: &'a str,
    /// Tipo.
    pub kind: ParamKind<'a>,
    /// Obrigatório?
    pub required: bool,
    /// Descrição que ensina ("o caminho alvo"; nunca vazia).
    pub description: &'a str,
}

/// Esquema declarativo de uma tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolSchema<'a> {
    /// Nome estável (verbo-substantivo, `snake_case`).
    pub name: &'a str,
    /// Descrição "Use when X. Do not use for Y." (< [`MAX_DESCRIPTION_CHARS`] caracteres).
    pub description: &'a str,
    /// Parâmetros.
    pub params: &'a [ParamSpec<'a>],
}

/// Valida um esquema e devolve todos os problemas (agregados, com o caminho exato).
#[must_use]
pub fn lint(schema: &ToolSchema<'_>) -> Issues {
    let mut issues = Vec::new();
    check_name(schema.name, &mut issues);
    check_description(schema.name, schema.description, &mut issues);
    check_params(schema, &mut issues);
    Issues::new(issues)
}

/// Valida **todos** os esquemas do registry, com o caminho prefixado por `tools[i]`.
#[must_use]
pub fn lint_all() -> Issues {
    let mut issues = Vec::new();
    for (index, schema) in SCHEMAS.iter().enumerate() {
        let report = lint(schema);
        for issue in report.as_slice() {
            issues.push(Issue::new(
                format!("tools[{index}].{}", issue.path),
                issue.message.clone(),
            ));
        }
    }
    Issues::new(issues)
}

/// Nome: `snake_case`, não vazio, e a primeira palavra tem de ser um verbo do conjunto fechado.
fn check_name(name: &str, issues: &mut Vec<Issue>) {
    if name.is_empty() {
        issues.push(Issue::new("name", "nome obrigatório em falta"));
        return;
    }
    if !is_snake_case(name) {
        issues.push(Issue::new(
            "name",
            format!("`{name}` não é `snake_case` (ex.: `read_file`)"),
        ));
    }
    if contains_poison(name) {
        issues.push(Issue::new(
            "name",
            "marcador de *poisoning* no nome (remova `<system>`/`ignore previous`/comentários)",
        ));
    }
    let verb = name.split('_').next().unwrap_or("");
    if !VERBS.contains(&verb) {
        issues.push(Issue::new(
            "name",
            format!("`{name}` não começa por um verbo conhecido (ex.: `read`, `write`, `find`)"),
        ));
    }
}

/// Descrição: obrigatória, `Use when … Do not use for …`, com teto e sem *poisoning*.
fn check_description(name: &str, description: &str, issues: &mut Vec<Issue>) {
    let path = format!("{name}.description");
    if description.is_empty() {
        issues.push(Issue::new(
            path,
            "descrição obrigatória. Exemplo: \"Use when X. Do not use for Y.\"",
        ));
        return;
    }
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        issues.push(Issue::new(
            path.clone(),
            format!("descrição excede {MAX_DESCRIPTION_CHARS} caracteres"),
        ));
    }
    if !description.contains("Use when") {
        issues.push(Issue::new(
            path.clone(),
            "descrição tem de começar por \"Use when …\" (o modelo decide pelo uso)",
        ));
    }
    if !description.contains("Do not use for") {
        issues.push(Issue::new(
            path.clone(),
            "descrição tem de dizer \"Do not use for …\" (evita uso errado)",
        ));
    }
    if contains_poison(description) {
        issues.push(Issue::new(
            path,
            "marcador de *poisoning* na descrição (remova `<system>`/`ignore previous`/comentários)",
        ));
    }
}

/// Parâmetros: nomes únicos `snake_case`, descrição não vazia, enums fechados com `pattern` para ids.
fn check_params(schema: &ToolSchema<'_>, issues: &mut Vec<Issue>) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for param in schema.params {
        let path = format!("{}.params.{}", schema.name, param.name);
        if param.name.is_empty() {
            issues.push(Issue::new(path, "nome de parâmetro obrigatório em falta"));
            continue;
        }
        if !seen.insert(param.name) {
            issues.push(Issue::new(path.clone(), "parâmetro duplicado"));
        }
        if !is_snake_case(param.name) {
            issues.push(Issue::new(
                path.clone(),
                format!("`{}` não é `snake_case`", param.name),
            ));
        }
        if param.description.is_empty() {
            issues.push(Issue::new(
                path.clone(),
                "descrição obrigatória (o modelo precisa de saber o que passar)",
            ));
        }
        if contains_poison(param.name) || contains_poison(param.description) {
            issues.push(Issue::new(
                path.clone(),
                "marcador de *poisoning* no parâmetro",
            ));
        }
        if let ParamKind::Enum(values) = param.kind {
            if values.is_empty() {
                issues.push(Issue::new(
                    path.clone(),
                    "`enum` sem valores: um conjunto fechado não pode ser vazio",
                ));
            }
            let mut unique: BTreeSet<&str> = BTreeSet::new();
            if values.iter().any(|value| !unique.insert(value)) {
                issues.push(Issue::new(path.clone(), "`enum` com valores duplicados"));
            }
        }
        if let ParamKind::Id(pattern) = param.kind
            && pattern.is_empty()
        {
            issues.push(Issue::new(
                path,
                "id tipado exige um `pattern` não vazio (ex.: `^[a-z0-9_]{1,32}$`)",
            ));
        }
    }
}

/// `true` se o texto contém um marcador de *poisoning* (comparação em minúsculas).
fn contains_poison(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    POISON.iter().any(|marker| lowered.contains(marker))
}

/// `true` para `snake_case` simples (minúsculas/dígitos, sem `__` nem extremos `_`).
fn is_snake_case(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('_')
        && !name.ends_with('_')
        && !name.contains("__")
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}
