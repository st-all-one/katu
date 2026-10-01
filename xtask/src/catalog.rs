//! `check-catalog` (E14-T06) — catálogos gerados do código e verificados.
//!
//! Gera `wiki/_ref/docs/catalog.md` (tools, regras, eventos de diag e ADRs) a partir das **fontes reais** e
//! falha se o ficheiro commitado divergir (um catálogo reescrito à mão é detetado). Regenerar:
//! `KATU_GEN_DOCS=1 cargo run -p xtask -- check-catalog`.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use katu_core::diag::events;
use katu_policy::{RuleCategory, RuleSet};
use katu_tools::schema::SCHEMAS;

use crate::docs::ADR_DIR;
use crate::walk::collect_rule_files;

/// Ficheiro gerado (versionado).
const CATALOG_FILE: &str = "wiki/_ref/docs/catalog.md";

/// Linha de regra: `(id, categoria, enunciado, remédio)`.
type RuleRow = (String, String, String, String);

/// Linha de ADR: `(ficheiro, título)`.
type AdrRow = (String, String);

/// Ponto de entrada de `check-catalog`.
#[allow(
    clippy::disallowed_methods,
    reason = "regeneração explícita por env var (`KATU_GEN_DOCS`), dev-only"
)]
pub(crate) fn check_catalog() -> Result<(), String> {
    let generated = render()?;
    if std::env::var("KATU_GEN_DOCS").as_deref() == Ok("1") {
        fs::write(CATALOG_FILE, &generated)
            .map_err(|err| format!("escrevendo {CATALOG_FILE}: {err}"))?;
        return Ok(());
    }
    let committed =
        fs::read_to_string(CATALOG_FILE).map_err(|err| format!("lendo {CATALOG_FILE}: {err}"))?;
    if committed == generated {
        Ok(())
    } else {
        Err(format!(
            "check-catalog falhou: {CATALOG_FILE} desatualizado; \
             corra `KATU_GEN_DOCS=1 cargo run -p xtask -- check-catalog`"
        ))
    }
}

/// Renderiza o catálogo completo (determinístico).
fn render() -> Result<String, String> {
    let mut out = String::new();
    out.push_str("# Catálogo (gerado)\n\n");
    out.push_str(
        "> **Gerado** por `cargo xtask check-catalog`; não editar à mão. \
         Regenerar: `KATU_GEN_DOCS=1 cargo run -p xtask -- check-catalog`.\n\n",
    );

    write(&mut out, format_args!("## Tools ({})\n\n", SCHEMAS.len()))?;
    out.push_str("| tool | descrição |\n| --- | --- |\n");
    for schema in SCHEMAS {
        write(
            &mut out,
            format_args!(
                "| `{}` | {} |\n",
                schema.name,
                escape_cell(schema.description)
            ),
        )?;
    }
    out.push('\n');

    let rules = collect_rules()?;
    write(&mut out, format_args!("## Regras ({})\n\n", rules.len()))?;
    out.push_str("| id | categoria | enunciado | remédio (Q-08) |\n| --- | --- | --- | --- |\n");
    for (id, category, statement, remedy) in &rules {
        write(
            &mut out,
            format_args!(
                "| `{id}` | {category} | {} | {} |\n",
                escape_cell(statement),
                escape_cell(remedy)
            ),
        )?;
    }
    out.push('\n');

    write(
        &mut out,
        format_args!("## Eventos de diag ({})\n\n", events::ALL.len()),
    )?;
    for id in events::ALL {
        write(&mut out, format_args!("- `{id}`\n"))?;
    }
    out.push('\n');

    let adrs = list_adrs()?;
    write(&mut out, format_args!("## ADRs ({})\n\n", adrs.len()))?;
    for (file, title) in &adrs {
        // O catálogo vive em `wiki/_ref/docs/`; o link é relativo a esse directório.
        write(&mut out, format_args!("- [{title}](../adr/{file})\n"))?;
    }
    Ok(out)
}

/// Escreve um fragmento no `String` (nunca falha, mas o lint exige tratar o `Result`).
fn write(out: &mut String, args: std::fmt::Arguments<'_>) -> Result<(), String> {
    out.write_fmt(args).map_err(|err| format!("render: {err}"))
}

/// Regras `Enforced`/`Advisory` de `policy/*.toml`, ordenadas por id.
fn collect_rules() -> Result<Vec<RuleRow>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rule_files(Path::new("policy"), &mut files)?;
    files.sort();
    let mut rules: Vec<RuleRow> = Vec::new();
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let set = RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        for rule in &set.rules {
            rules.push((
                rule.id.as_str().to_string(),
                category_name(rule.category).to_string(),
                rule.statement.clone(),
                rule.remedy.clone().unwrap_or_default(),
            ));
        }
    }
    rules.sort();
    Ok(rules)
}

/// Nome estável de uma categoria.
fn category_name(category: RuleCategory) -> &'static str {
    match category {
        RuleCategory::Enforced => "enforced",
        RuleCategory::Advisory => "advisory",
        RuleCategory::Perception => "perception",
        _ => "other",
    }
}

/// ADRs `wiki/_ref/adr/NNNN-*.md` com o respetivo título (H1), ordenadas.
fn list_adrs() -> Result<Vec<AdrRow>, String> {
    let dir = Path::new(ADR_DIR);
    let entries = fs::read_dir(dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
    let mut adrs: Vec<AdrRow> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let numbered = name
            .get(0..4)
            .is_some_and(|prefix| prefix.bytes().all(|byte| byte.is_ascii_digit()));
        let is_md = Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !numbered || !is_md {
            continue;
        }
        let path = entry.path();
        let text =
            fs::read_to_string(&path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
        let title = text
            .lines()
            .find_map(|line| {
                line.strip_prefix("# ")
                    .map(|title| title.trim().to_string())
            })
            .unwrap_or_else(|| name.to_string());
        adrs.push((name.to_string(), title));
    }
    adrs.sort();
    Ok(adrs)
}

/// Escapa uma célula de tabela markdown (o `|` quebraria a grelha).
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::escape_cell;

    #[test]
    fn pipes_are_escaped() {
        assert_eq!(escape_cell("a | b"), "a \\| b");
    }
}
