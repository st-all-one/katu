//! Tool `search` (E06-T05): `grep`/`find`/`ls` com saída AI-first (DF12).
//!
//! Varredura **determinística** (ordem canónica, sem `HashMap`), com limite de resultados e
//! informação negativa. O escopo é responsabilidade da política (E07); aqui só se lê sob a raiz
//! resolvida. Saída: `ToolReport` renderizado em TOON (JSON como alternativa).

mod grep;
mod map;
mod walk;

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::Fs;
use katu_policy::{ControlId, ResolvedPath, SearchMode, ToolArgs, ToolName, ToolUse};

/// Limite por omissão de resultados.
pub const DEFAULT_LIMIT: usize = 200;

/// Executor de busca (`grep`/`find`/`ls`).
pub struct SearchTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Limite de resultados.
    pub limit: usize,
}

/// Constrói o `ToolUse` de busca com a **raiz resolvida** em `resolved_paths` (E07-T05), para a
/// política avaliar `DenyRead`/`DenySensitiveRead` sobre o que a busca vai varrer.
#[must_use]
pub fn search_use(root: &ResolvedPath, query: impl Into<String>, mode: SearchMode) -> ToolUse {
    ToolUse {
        name: ToolName::Search,
        args: ToolArgs::Search {
            root: root.clone(),
            query: query.into(),
            mode,
        },
        resolved_paths: vec![root.clone()],
        argv: None,
        cwd: root.clone(),
    }
}

impl Tool for SearchTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Search
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_SEARCH);
        let ToolArgs::Search { root, query, mode } = &use_.args else {
            return unavailable("search");
        };
        let path = Path::new(root.as_str());
        if !self.fs.exists(path) {
            return unavailable("missing");
        }
        let limit = self.limit.max(1);
        let report = match mode {
            SearchMode::Grep => grep::run(self.fs, path, query, limit),
            SearchMode::Find => map::find(self.fs, path, query, limit),
            SearchMode::Ls => map::ls(self.fs, path, limit),
            _ => return unavailable("mode"),
        };
        ToolOutput::report(report)
    }
}

fn unavailable(control: &'static str) -> ToolOutput {
    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests;
