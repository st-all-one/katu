//! Skills do projeto (E20-T13): descoberta de `.agents/skill{,s}/*/SKILL.md`.
//!
//! Um *skill* é um diretório com `SKILL.md` e frontmatter `name`/`description`. A descoberta é
//! **fail-open**: sem `.agents/` devolve lista vazia; um `SKILL.md` malformado ou sem descrição é
//! ignorado (nunca quebra o arranque). O catálogo entra no prompt de sistema (nome, descrição e
//! caminho) e o modelo lê o `SKILL.md` com a tool `read` quando a tarefa o pedir.

use std::path::{Path, PathBuf};

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
    let rest = content.strip_prefix("---")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    rest.get(..rest.find("\n---")?)
}

/// Valor de uma chave escalar (inline, ou `>`/`|` dobrado nas linhas indentadas seguintes).
fn scalar(front: &str, key: &str) -> Option<String> {
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
#[must_use]
pub fn catalog(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "skills do projeto (lê o `SKILL.md` com a tool `read` quando a tarefa o pedir):\n",
    );
    for skill in skills {
        text.push_str("- ");
        text.push_str(&skill.name);
        text.push_str(" — ");
        text.push_str(&skill.description);
        text.push_str(" (");
        text.push_str(&skill.path.display().to_string());
        text.push_str(")\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Skill, catalog, discover, parse};
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
        assert!(catalog(&[]).is_empty());
    }

    #[test]
    fn catalog_lists_name_description_and_path() {
        let skills = vec![Skill {
            name: "rust".to_string(),
            description: "Rust.".to_string(),
            path: std::path::PathBuf::from("/p/.agents/skill/rust/SKILL.md"),
        }];
        let text = catalog(&skills);
        assert!(text.contains("- rust — Rust."), "{text}");
        assert!(text.contains("SKILL.md"), "{text}");
    }
}
