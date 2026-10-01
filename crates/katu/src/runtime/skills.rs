//! Contexto do projeto (E20-T13): `AGENTS.md` (fonte de verdade) e skills acionáveis.
//!
//! Vive num módulo filho para manter `runtime.rs` sob o teto. A leitura é **fail-open**: ausência
//! de `AGENTS.md` ou de `.agents/` não quebra o arranque.

use std::path::Path;

use katu_core::ports::Fs;
use katu_core::prompt::condense;
use katu_core::skill::{Skill, catalog, discover};

use super::Runtime;

/// Nome do ficheiro de instruções do projeto (fonte de verdade máxima).
pub(crate) const AGENTS_FILE: &str = "AGENTS.md";

/// Lê o `AGENTS.md` da raiz (ausência ou bytes inválidos → `None`).
///
/// O texto é **condensado** (`katu_core::prompt::condense`, Q-19): a sintaxe redundante do router
/// (link com texto igual ao alvo, negrito) sai; texto, alvos e estrutura ficam. Como a condensação
/// acontece **aqui**, o `ProjectContext` loga exatamente o que o modelo vê (Q-16).
pub(crate) fn read_instructions(fs: &dyn Fs, root: &Path) -> Option<String> {
    let _span = katu_core::trace_fn!("runtime::skills::read_instructions");

    let bytes = fs.read(&root.join(AGENTS_FILE)).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let condensed = condense(text.trim());
    let trimmed = condensed.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Descobre as skills do projeto (fail-open).
pub(crate) fn load_skills(fs: &dyn Fs, root: &Path) -> Vec<Skill> {
    let _span = katu_core::trace_fn!("runtime::skills::load_skills");

    discover(fs, root)
}

impl Runtime<'_> {
    /// Instruções do projeto (`AGENTS.md`), se existirem (E20-T13).
    #[must_use]
    pub(crate) fn instructions(&self) -> Option<&str> {
        let _span = katu_core::trace_fn!("runtime::skills::instructions");

        self.instructions.as_deref()
    }

    /// Skill pelo nome (E20-T13).
    #[must_use]
    pub(crate) fn skill(&self, name: &str) -> Option<&Skill> {
        let _span = katu_core::trace_fn!("runtime::skills::skill");

        self.skills.iter().find(|skill| skill.name == name)
    }

    /// Catálogo de skills para o prompt de sistema (vazio se não houver).
    #[must_use]
    pub(crate) fn skills_catalog(&self) -> String {
        let _span = katu_core::trace_fn!("runtime::skills::skills_catalog");

        catalog(&self.skills, &self.goal, self.session.root())
    }
}
