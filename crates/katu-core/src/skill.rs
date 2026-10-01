//! Skills do projeto (E20-T13): descoberta de `.agents/skill{,s}/*/SKILL.md`.
//!
//! Um *skill* é um diretório com `SKILL.md` e frontmatter `name`/`description`. A descoberta é
//! **fail-open**: sem `.agents/` devolve lista vazia; um `SKILL.md` malformado ou sem descrição é
//! ignorado (nunca quebra o arranque). O catálogo entra no prompt de sistema (nome, **primeira
//! frase** da descrição e caminho) e o modelo lê o `SKILL.md` com a tool `read` quando a tarefa o
//! pedir.

use std::path::{Path, PathBuf};

use crate::diag::{Level, events};
use crate::ports::Fs;

/// Diretórios de skills suportados (singular e plural, E20-T13), em ordem canônica.
pub const SKILL_DIRS: [&str; 2] = [".agents/skill", ".agents/skills"];

/// Skill descoberto no projeto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Nome (frontmatter `name`; queda para o nome do diretório).
    pub name: String,
    /// Descrição (frontmatter `description`); vazia → não carregado.
    pub description: String,
    /// Caminho do `SKILL.md`.
    pub path: PathBuf,
}

/// Descobre skills sob a raiz do projeto (ordem canônica; o primeiro nome vence).
#[must_use]
pub fn discover(fs: &dyn Fs, root: &Path) -> Vec<Skill> {
    let _span = crate::fn_span!(Level::Debug, events::SKILL_DISCOVER, "skill::discover");
    let mut skills: Vec<Skill> = Vec::new();
    for dir in SKILL_DIRS {
        let Ok(entries) = fs.list_dir(&root.join(dir)) else {
            continue;
        };
        for entry in entries {
            if !fs.is_dir(&entry) {
                continue;
            }
            let file = entry.join("SKILL.md");
            let Ok(bytes) = fs.read(&file) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            if let Some(skill) = parse(text, &file, &entry)
                && !skills.iter().any(|known| known.name == skill.name)
            {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

/// Lê `name`/`description` do frontmatter. Sem descrição → `None` (não carregado).
#[must_use]
pub fn parse(content: &str, path: &Path, dir: &Path) -> Option<Skill> {
    let _span = crate::fn_span!(Level::Trace, events::SKILL_READ, "skill::parse");
    let front = frontmatter(content)?;
    let fallback = dir.file_name()?.to_string_lossy().into_owned();
    let name = scalar(front, "name").unwrap_or(fallback);
    if name.is_empty() {
        return None;
    }
    let description = scalar(front, "description")?;
    Some(Skill {
        name,
        description,
        path: path.to_path_buf(),
    })
}

/// Bloco de frontmatter (entre o primeiro `---` e o `---` seguinte).
fn frontmatter(content: &str) -> Option<&str> {
    let _span = crate::trace_fn!("skill::frontmatter");

    let rest = content.strip_prefix("---")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    rest.get(..rest.find("\n---")?)
}

/// Valor de uma chave escalar (inline, ou `>`/`|` dobrado nas linhas indentadas seguintes).
fn scalar(front: &str, key: &str) -> Option<String> {
    let _span = crate::trace_fn!("skill::scalar");

    let prefix = format!("{key}:");
    let mut lines = front.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let rest = rest.trim();
        if rest == ">" || rest == "|" {
            let mut parts: Vec<&str> = Vec::new();
            for next in lines.by_ref() {
                if next.starts_with(' ') || next.starts_with('\t') {
                    let trimmed = next.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed);
                    }
                } else {
                    break;
                }
            }
            let value = parts.join(" ");
            return (!value.is_empty()).then_some(value);
        }
        return (!rest.is_empty()).then(|| rest.to_owned());
    }
    None
}

/// Catálogo de skills para o prompt de sistema (vazio se não houver skills).
///
/// Cada linha traz **nome, primeira frase da descrição e caminho relativo à raiz do projeto** — o
/// catálogo serve para o modelo decidir se lê o `SKILL.md`, não para o substituir. A descrição
/// completa custava 4828 B (~1330 tokens, 37 % do prompt, §1.5 do `OPTIMIZATION_PLAN`). O caminho
/// é relativo (e não absoluto) para não gastar tokens com o prefixo da máquina nem tornar o prompt
/// dependente do *checkout* (o gate `xtask gate:prompt` mede-o). A ordem é **por relevância** face
/// ao objetivo (empate pelo nome), determinística: nenhuma skill é omitida.
#[must_use]
pub fn catalog(skills: &[Skill], goal: &str, root: &Path) -> String {
    let _span = crate::trace_fn!("skill::catalog");

    if skills.is_empty() {
        return String::new();
    }
    let mut ranked: Vec<(&Skill, usize)> = skills
        .iter()
        .map(|skill| (skill, relevance(skill, goal)))
        .collect();
    ranked.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut text = String::from(
        "skills do projeto (lê o `SKILL.md` com a tool `read` quando a tarefa o pedir):\n",
    );
    for (skill, _) in ranked {
        text.push_str("- ");
        text.push_str(&skill.name);
        text.push_str(" — ");
        text.push_str(&hint(&skill.description));
        text.push_str(" (");
        text.push_str(&relative_path(&skill.path, root));
        text.push_str(")\n");
    }
    text
}

/// Caminho do `SKILL.md` relativo à raiz do projeto (o absoluto, se não estiver sob a raiz).
///
/// Pública para o gate `xtask gate:prompt` calcular a baseline com a **mesma** renderização.
#[must_use]
pub fn relative_path(path: &Path, root: &Path) -> String {
    let _span = crate::trace_fn!("skill::relative_path");

    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Teto da descrição no catálogo (caracteres).
///
/// Calibrado em §1.5: com 8 skills, o teto paga ~100 B por skill em vez de parágrafos inteiros,
/// sem perder o sinal de "quando carregar" (o detalhe fica no `SKILL.md`).
pub const MAX_HINT_CHARS: usize = 100;

/// Primeira frase da descrição, normalizada e cortada ao teto na fronteira de palavra.
///
/// Determinística: mesma descrição → mesmos bytes. O corte nunca parte um `char`.
#[must_use]
pub fn hint(description: &str) -> String {
    let _span = crate::trace_fn!("skill::hint");

    let flat = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let sentence = flat.find(". ").map_or(flat.as_str(), |end| {
        flat.get(..end.saturating_add(1)).unwrap_or(flat.as_str())
    });
    if sentence.chars().count() <= MAX_HINT_CHARS {
        return sentence.to_string();
    }
    // Reserva um `char` para a elipse: o resultado nunca excede o teto.
    let ceiling = MAX_HINT_CHARS.saturating_sub(1);
    let boundary = sentence
        .char_indices()
        .map(|(index, ch)| index.saturating_add(ch.len_utf8()))
        .take_while(|end| *end <= ceiling)
        .last()
        .unwrap_or(0);
    let prefix = sentence.get(..boundary).unwrap_or("");
    let cut = prefix
        .char_indices()
        .rfind(|(_, ch)| ch.is_whitespace())
        .map_or(boundary, |(index, _)| index);
    format!("{}…", sentence.get(..cut).unwrap_or(""))
}

/// Sobreposição entre os termos do objetivo e o nome/descrição da skill (determinística).
fn relevance(skill: &Skill, goal: &str) -> usize {
    let _span = crate::trace_fn!("skill::relevance");

    let haystack = format!("{} {}", skill.name, skill.description).to_lowercase();
    goal.split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| term.chars().count() >= 4)
        .map(str::to_lowercase)
        .filter(|term| haystack.contains(term.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{MAX_HINT_CHARS, Skill, catalog, discover, hint, parse};
    use crate::ports::Fs as _;
    use crate::ports::MemFs;

    const SKILL: &str = "---\nname: rust\ndescription: >\n  Rust moderno.\n  Usa quando escreves Rust.\ncategory: languages\n---\n\n# Rust\n";

    #[test]
    fn parses_folded_frontmatter() {
        let parsed = parse(
            SKILL,
            Path::new("/p/.agents/skill/rust/SKILL.md"),
            Path::new("/p/.agents/skill/rust"),
        );
        assert_eq!(
            parsed.as_ref().map(|skill| skill.name.as_str()),
            Some("rust")
        );
        assert_eq!(
            parsed.map(|skill| skill.description),
            Some("Rust moderno. Usa quando escreves Rust.".to_string())
        );
    }

    #[test]
    fn without_description_is_not_loaded() {
        let content = "---\nname: x\n---\n";
        assert!(parse(content, Path::new("/p/SKILL.md"), Path::new("/p/x")).is_none());
    }

    #[test]
    fn name_falls_back_to_the_directory() {
        let content = "---\ndescription: só descrição\n---\n";
        let parsed = parse(content, Path::new("/p/SKILL.md"), Path::new("/p/meu-skill"));
        assert_eq!(
            parsed.map(|skill| skill.name),
            Some("meu-skill".to_string())
        );
    }

    #[test]
    fn discovers_both_skill_dirs_and_dedupes() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let root = Path::new("/p");
        for (dir, body) in [
            (".agents/skill/rust", SKILL),
            (
                ".agents/skills/grpc",
                "---\nname: grpc\ndescription: gRPC.\n---\n",
            ),
            (
                ".agents/skills/rust",
                "---\nname: rust\ndescription: duplicado.\n---\n",
            ),
        ] {
            let path = root.join(dir).join("SKILL.md");
            if let Some(parent) = path.parent() {
                fs.create_dir_all(parent)?;
            }
            fs.write_atomic(&path, body.as_bytes())?;
        }
        let skills = discover(&fs, root);
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(names, ["grpc", "rust"]);
        assert_eq!(
            skills.first().map(|skill| skill.name.as_str()),
            Some("grpc")
        );
        Ok(())
    }

    #[test]
    fn absent_tree_is_empty() {
        let fs = MemFs::new();
        assert!(discover(&fs, Path::new("/nada")).is_empty());
        assert!(catalog(&[], "", Path::new("/p")).is_empty());
    }

    #[test]
    fn catalog_lists_name_description_and_path() {
        let skills = vec![Skill {
            name: "rust".to_string(),
            description: "Rust.".to_string(),
            path: std::path::PathBuf::from("/p/.agents/skill/rust/SKILL.md"),
        }];
        let text = catalog(&skills, "escrever código", Path::new("/p"));
        assert!(text.contains("- rust — Rust."), "{text}");
        // O caminho é **relativo** à raiz: sem o prefixo da máquina no prompt.
        assert!(text.contains("(.agents/skill/rust/SKILL.md)"), "{text}");
    }

    #[test]
    fn hint_is_one_sentence_and_never_exceeds_the_ceiling() {
        assert_eq!(
            hint("Rust moderno. Usa quando escreves Rust."),
            "Rust moderno."
        );
        assert_eq!(hint("  um   só   termo  "), "um só termo");
        let long = format!("{} palavra", "a".repeat(300));
        let cut = hint(&long);
        assert!(cut.chars().count() <= MAX_HINT_CHARS, "{cut}");
        assert!(cut.ends_with('…'), "{cut}");
    }

    #[test]
    fn catalog_orders_by_relevance_without_dropping_any_skill() {
        let skills = vec![
            Skill {
                name: "git-daily".to_string(),
                description: "Git do dia-a-dia.".to_string(),
                path: std::path::PathBuf::from("/p/.agents/skill/git-daily/SKILL.md"),
            },
            Skill {
                name: "grpc".to_string(),
                description: "gRPC sobre HTTP/2.".to_string(),
                path: std::path::PathBuf::from("/p/.agents/skill/grpc/SKILL.md"),
            },
        ];
        let text = catalog(&skills, "investigar com git bisect", Path::new("/p"));
        let first = text.lines().nth(1).unwrap_or_default();
        assert!(first.contains("git-daily"), "{text}");
        assert!(text.contains("grpc"), "{text}");
        assert_eq!(
            text,
            catalog(&skills, "investigar com git bisect", Path::new("/p"))
        );
    }
}
