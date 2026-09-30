//! Registo de esquema do formato ao modelo (ADR 0005, emenda v3).
//!
//! Com os headers removidos do *stream*, o **prime** é o registo: declara, por secção, o modo
//! (tabela/linhas ou bloco literal) e as colunas por ordem. Fonte única de verdade: a projeção e o
//! prime leem daqui, pelo que não há drift entre o que é emitido e o que é ensinado.

mod registry;

use registry::REGISTRY;

/// Domínio fechado de uma coluna (`&[]` = livre).
pub type Domain = &'static [&'static str];

/// Coluna: nome estável e domínio fechado opcional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    /// Nome da coluna (curto mas claro).
    pub name: &'static str,
    /// Domínio fechado (enums) ou vazio.
    pub domain: Domain,
}

/// Modo de uma secção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Tabela de linhas (`\x1e`), células separadas por `\x1f`.
    Rows,
    /// Bloco literal (`\x1d`): linhas cruas, sem células.
    Literal,
}

/// Esquema de uma secção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    /// Nome da secção.
    pub name: &'static str,
    /// Modo.
    pub mode: Mode,
    /// Colunas (vazio em modo literal).
    pub cols: &'static [ColumnSpec],
}

/// Registo de todas as secções emitidas (modo + colunas por ordem).
#[must_use]
pub fn registry() -> &'static [TableSpec] {
    let _span = crate::trace_fn!("toon::schema::registry");

    REGISTRY
}

/// Esquema de uma secção pelo nome (inclui filhos qualificados, ex.: `clusters.hits`).
#[must_use]
pub fn spec(name: &str) -> Option<&'static TableSpec> {
    let _span = crate::trace_fn!("toon::schema::spec");

    REGISTRY.iter().find(|spec| spec.name == name)
}

/// Valida o registo: nomes únicos/não vazios, colunas não vazias e literais sem colunas.
///
/// É o gate que impede uma secção nova de sair sem esquema (o prime deixaria de a ensinar).
#[must_use]
pub fn validate() -> Vec<&'static str> {
    let _span = crate::trace_fn!("toon::schema::validate");

    let mut issues = Vec::new();
    for (index, spec) in REGISTRY.iter().enumerate() {
        if spec.name.is_empty() {
            issues.push("secção sem nome");
        }
        if REGISTRY
            .iter()
            .take(index)
            .any(|other| other.name == spec.name)
        {
            issues.push("nome de secção duplicado");
        }
        if spec.mode == Mode::Literal && !spec.cols.is_empty() {
            issues.push("bloco literal com colunas");
        }
        if spec.mode == Mode::Rows && spec.cols.is_empty() {
            issues.push("tabela sem colunas");
        }
        for col in spec.cols {
            if col.name.is_empty() {
                issues.push("coluna sem nome");
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests;
