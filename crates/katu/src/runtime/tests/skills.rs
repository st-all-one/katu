//! Testes do contexto do projeto (E20-T13): `AGENTS.md` e skills, fail-open.

use katu_core::kernel::{Event, read_records};
use katu_core::ports::{FixedClock, Timestamp};

use super::root;
use crate::ports::StdFs;
use crate::runtime::Runtime;

#[test]
fn loads_agents_md_and_skills() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("contexto")?;
    std::fs::write(root.join("AGENTS.md"), "regra máxima\n")?;
    let dir = root.join(".agents/skill/rust");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: rust\ndescription: Rust moderno.\n---\n",
    )?;

    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    assert_eq!(runtime.instructions(), Some("regra máxima"));
    assert_eq!(
        runtime.skill("rust").map(|skill| skill.name.as_str()),
        Some("rust")
    );
    assert!(runtime.skills_catalog().contains("rust"));

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn absent_context_is_fail_open() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("sem-contexto")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    assert!(runtime.instructions().is_none());
    assert!(runtime.skill("rust").is_none());
    assert!(runtime.skills_catalog().is_empty());

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Q-16: o prompt de sistema é reconstruível do log (`Model-visible ⟺ logged`, E04).
#[test]
fn the_project_context_is_logged_for_replay() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("contexto-logado")?;
    std::fs::write(root.join("AGENTS.md"), "regra máxima\n")?;
    let dir = root.join(".agents/skill/rust");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: rust\ndescription: Rust moderno.\n---\n",
    )?;

    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let runtime = Runtime::open(&fs, &clock, &root, "objetivo")?;
    let catalog = runtime.skills_catalog();
    let path = runtime.session().log_path().to_path_buf();
    // Evento de controlo: não entra no histórico do modelo.
    assert!(runtime.messages()?.is_empty());
    drop(runtime);

    let (agents, skills) = read_records(&fs, &path)?
        .into_iter()
        .find_map(|record| match record.event {
            Event::ProjectContext { agents, skills } => Some((agents, skills)),
            _ => None,
        })
        .ok_or("o log tem de conter `ProjectContext`")?;
    assert_eq!(agents.as_deref(), Some("regra máxima"));
    assert_eq!(skills.as_deref(), Some(catalog.as_str()));

    std::fs::remove_dir_all(&root)?;
    Ok(())
}
