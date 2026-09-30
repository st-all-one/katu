//! Contexto do projeto (E20-T13): `AGENTS.md` (fonte de verdade) e skills acionáveis.
//!
//! Vive num módulo filho para manter `runtime.rs` sob o teto. A leitura é **fail-open**: ausência
//! de `AGENTS.md` ou de `.agents/` não quebra o arranque.

use std::path::Path;

use katu_core::ports::Fs;
use katu_core::skill::{Skill, catalog, discover};

use super::Runtime;

/// Nome do ficheiro de instruções do projeto (fonte de verdade máxima).
pub(crate) const AGENTS_FILE: &str = "AGENTS.md";

/// Lê o `AGENTS.md` da raiz (ausência ou bytes inválidos → `None`).
pub(crate) fn read_instructions(fs: &dyn Fs, root: &Path) -> Option<String> {
    let bytes = fs.read(&root.join(AGENTS_FILE)).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Descobre as skills do projeto (fail-open).
pub(crate) fn load_skills(fs: &dyn Fs, root: &Path) -> Vec<Skill> {
    discover(fs, root)
}

impl Runtime<'_> {
    /// Instruções do projeto (`AGENTS.md`), se existirem (E20-T13).
    #[must_use]
    pub(crate) fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Skill pelo nome (E20-T13).
    #[must_use]
    pub(crate) fn skill(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.name == name)
    }

    /// Catálogo de skills para o prompt de sistema (vazio se não houver).
    #[must_use]
    pub(crate) fn skills_catalog(&self) -> String {
        catalog(&self.skills)
    }
}
