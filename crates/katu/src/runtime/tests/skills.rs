//! Testes do contexto do projeto (E20-T13): `AGENTS.md` e skills, fail-open.

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
