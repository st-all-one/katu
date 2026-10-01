//! `check-docs` — ADRs com alternativas obrigatórias (E14-T01) e o router de regras do agente
//! (E14-T03: um facto, um lar).
//!
//! Uma decisão sem o que **venceu** convida a re-litigá-la (§44). O router (`AGENTS.md`) tem de
//! caber num orçamento de linhas e encaminhar, sem duplicar: títulos (H1) e corpos únicos na
//! superfície, tópicos sem órfãos.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::markdown_links;
use crate::walk::collect_by_extension;

/// Secção obrigatória em cada ADR.
const REQUIRED_SECTION: &str = "## Alternatives considered";

/// Orçamento do router `AGENTS.md` (E14-T03): um índice, não um manual.
pub(crate) const MAX_ROUTER_LINES: usize = 50;

/// Router do agente.
const ROUTER: &str = "AGENTS.md";
/// Índice de regras do agente (1.º salto do router).
const AGENT_RULES: &str = "wiki/_ref/docs/agent-rules.md";
/// Diretório de tópicos (2.º salto).
const TOPICS_DIR: &str = "wiki/_ref/docs/topics";
/// Directório das ADRs (o lar das decisões registadas).
pub(crate) const ADR_DIR: &str = "wiki/_ref/adr";
/// Directório dos planos (épicos e planos-maior).
pub(crate) const PLAN_DIR: &str = "wiki/_ref/plan";
/// Directório do wiki novo (proposição, método).
pub(crate) const WIKI_DIR: &str = "wiki";
/// Directório da documentação de referência (regras, catálogo, CLI, tópicos, postmortems).
pub(crate) const DOCS_DIR: &str = "wiki/_ref/docs";

/// Verifica as secções obrigatórias de todas as ADRs.
///
/// # Erros
/// Mensagem agregada com todas as ADRs em falta; `Ok(())` se `docs/adr/` ainda não existir.
pub(crate) fn check_adrs() -> Result<(), String> {
    let dir = Path::new(ADR_DIR);
    if !dir.exists() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_by_extension(dir, "md", &mut files)?;
    let mut violations: Vec<String> = Vec::new();
    for file in files {
        if is_index(&file) {
            continue;
        }
        let source =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        if !source.contains(REQUIRED_SECTION) {
            violations.push(format!("{}: falta `{REQUIRED_SECTION}`", file.display()));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-docs falhou (ADR sem alternativas; E14-T01):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Verifica o router, o índice de regras e os tópicos (E14-T03).
///
/// # Erros
/// Mensagem agregada com orçamento excedido, conteúdo ausente, duplicação ou tópico órfão;
/// `Ok(())` enquanto não existir `AGENTS.md`.
pub(crate) fn check_agent_rules() -> Result<(), String> {
    if !Path::new(ROUTER).exists() {
        return Ok(());
    }
    let router = read(ROUTER)?;
    let rules = read(AGENT_RULES)?;
    let mut topics: Vec<(String, String)> = Vec::new();
    if Path::new(TOPICS_DIR).is_dir() {
        let mut files = Vec::new();
        collect_by_extension(Path::new(TOPICS_DIR), "md", &mut files)?;
        files.sort();
        for file in files {
            let name = file.display().to_string();
            topics.push((name.clone(), read(&name)?));
        }
    }
    let mut violations = Vec::new();
    let lines = router.lines().count();
    if lines > MAX_ROUTER_LINES {
        violations.push(format!(
            "{ROUTER}: {lines} linhas > {MAX_ROUTER_LINES} (router é um índice; E14-T03)"
        ));
    }
    violations.extend(surface_violations(&router, &rules, &topics));
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-docs falhou (router; E14-T03):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Lê um ficheiro de texto.
fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("lendo {path}: {err}"))
}

/// Violações da superfície de regras (puro; testável sem ficheiros).
fn surface_violations(router: &str, rules: &str, topics: &[(String, String)]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut docs: Vec<(&str, &str)> = vec![(ROUTER, router), (AGENT_RULES, rules)];
    for (name, text) in topics {
        docs.push((name.as_str(), text.as_str()));
    }
    // 1. Ausência de conteúdo.
    for (name, text) in &docs {
        if !has_body(text) {
            violations.push(format!("{name}: sem título e/ou corpo (E14-T03)"));
        }
    }
    // 2. Um facto, um lar: títulos (H1) e corpos não repetidos.
    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    for (name, text) in &docs {
        if let Some(title) = first_heading(text)
            && let Some(other) = titles.insert(title.clone(), (*name).to_string())
            && other != *name
        {
            violations.push(format!("título duplicado `{title}`: {other} e {name}"));
        }
        if let Some(other) = bodies.insert(text.trim().to_string(), (*name).to_string())
            && other != *name
        {
            violations.push(format!("conteúdo idêntico: {other} e {name}"));
        }
    }
    // 3. O router liga ao índice de regras.
    if !links(router).iter().any(|target| target == AGENT_RULES) {
        violations.push(format!("{ROUTER}: não liga a {AGENT_RULES}"));
    }
    // 4. Tópicos órfãos (não referenciados pelo índice).
    let base = Path::new(AGENT_RULES)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let linked = links(rules);
    for (name, _) in topics {
        let reachable = linked
            .iter()
            .any(|target| base.join(target).to_string_lossy() == name.as_str());
        if !reachable {
            violations.push(format!("{name}: órfão (não referenciado em {AGENT_RULES})"));
        }
    }
    violations
}

/// Ligações internas (sem âncora, sem externas) de um markdown.
fn links(source: &str) -> Vec<String> {
    markdown_links(source)
        .into_iter()
        .filter_map(|link| {
            let target = link.split('#').next().unwrap_or("").trim();
            if target.is_empty() || target.starts_with("http") || target.starts_with("mailto:") {
                None
            } else {
                Some(target.to_string())
            }
        })
        .collect()
}

/// Primeiro título `# ` (H1) de um markdown.
fn first_heading(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|title| title.trim().to_string())
    })
}

/// `true` se o documento tem um título e pelo menos uma linha de corpo.
fn has_body(source: &str) -> bool {
    let mut has_title = false;
    let mut has_text = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            has_title = true;
        } else {
            has_text = true;
        }
    }
    has_title && has_text
}

/// `true` para o índice/template (`README.md`), que não é uma decisão.
pub(crate) fn is_index(file: &Path) -> bool {
    file.file_name().and_then(std::ffi::OsStr::to_str) == Some("README.md")
}

#[cfg(test)]
mod tests {
    use super::{first_heading, has_body, is_index, surface_violations};
    use std::path::Path;

    #[test]
    fn index_is_skipped() {
        assert!(is_index(Path::new("wiki/_ref/adr/README.md")));
        assert!(!is_index(Path::new("wiki/_ref/adr/0001-x.md")));
    }

    #[test]
    fn empty_document_has_no_body() {
        assert!(!has_body("# Título\n"));
        assert!(has_body("# Título\n\ntexto\n"));
    }

    #[test]
    fn first_heading_is_the_h1() {
        assert_eq!(first_heading("## Sub\n# Home\n"), Some("Home".to_string()));
        assert_eq!(first_heading("sem título\n"), None);
    }

    #[test]
    fn duplicate_titles_are_rejected() {
        let router = "# Router\n\n[regras](wiki/_ref/docs/agent-rules.md)\n";
        let rules = "# Regras\n\n- [x](topics/x.md)\n";
        let topics = vec![(
            "wiki/_ref/docs/topics/x.md".to_string(),
            "# Regras\n\ncorpo próprio\n".to_string(),
        )];
        let violations = surface_violations(router, rules, &topics);
        assert!(violations.iter().any(|v| v.contains("título duplicado")));
    }

    #[test]
    fn orphan_topic_is_rejected() {
        let router = "# Router\n\n[regras](wiki/_ref/docs/agent-rules.md)\n";
        let rules = "# Regras\n\nsem links\n";
        let topics = vec![(
            "wiki/_ref/docs/topics/x.md".to_string(),
            "# X\n\ncorpo\n".to_string(),
        )];
        let violations = surface_violations(router, rules, &topics);
        assert!(violations.iter().any(|v| v.contains("órfão")));
    }

    #[test]
    fn a_clean_surface_has_no_violations() {
        let router = "# Router\n\n[regras](wiki/_ref/docs/agent-rules.md)\n";
        let rules = "# Regras\n\n- [x](topics/x.md)\n";
        let topics = vec![(
            "wiki/_ref/docs/topics/x.md".to_string(),
            "# X\n\ncorpo\n".to_string(),
        )];
        assert!(surface_violations(router, rules, &topics).is_empty());
    }
}
