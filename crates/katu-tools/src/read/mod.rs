//! Tool `read` com **views** (E06-T03/DF12): devolve metadados e ponteiros, não um despejo.
//!
//! Views: `full` (conteúdo, truncado de forma determinística), `range`, `outline`, `summary`
//! (default), `symbol` e `diff` (delta contra `base`). A estrutura sai de uma heurística leve
//! ([`crate::outline`]); tree-sitter fica gated por medição.

mod views;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod truncation_tests;

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::Fs;
use katu_core::report::ToolReport;
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

pub use views::View;

/// Range de linhas (1-based, inclusivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// Primeira linha.
    pub start: u32,
    /// Última linha.
    pub end: u32,
}

/// Orçamento de leitura (truncagem **determinística**).
#[derive(Debug, Clone, Copy)]
pub struct ReadBudget {
    /// Máximo de linhas devolvidas.
    pub max_lines: usize,
    /// Máximo de bytes devolvidos.
    pub max_bytes: usize,
}

impl Default for ReadBudget {
    fn default() -> Self {
        let _span = katu_core::trace_fn!("read::default");

        Self {
            max_lines: 400,
            max_bytes: 24_000,
        }
    }
}

/// Tool de leitura com view/range/símbolo.
pub struct ReadTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// View pedida.
    pub view: View,
    /// Range (para [`View::Range`]).
    pub range: Option<LineRange>,
    /// Símbolo (para [`View::Symbol`]).
    pub symbol: Option<String>,
    /// Versão anterior do conteúdo (para [`View::Diff`]); sem base, a view fica indisponível.
    pub base: Option<String>,
    /// Orçamento de truncagem.
    pub budget: ReadBudget,
}

impl Tool for ReadTool<'_> {
    fn name(&self) -> ToolName {
        let _span = katu_core::trace_fn!("read::name");

        ToolName::Read
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_READ, "read::execute");
        let ToolArgs::Read { path } = &use_.args else {
            return unavailable("read");
        };
        let path_ref = Path::new(path.as_str());
        // Ficheiro inexistente: relatório **legível** (o modelo sabe que não adianta repetir).
        if !self.fs.exists(path_ref) {
            return missing(path.as_str());
        }
        let Ok(bytes) = self.fs.read(path_ref) else {
            return unavailable("read");
        };
        let text = String::from_utf8_lossy(&bytes);
        match views::build(views::Build {
            view: self.view,
            path: path.as_str(),
            text: &text,
            bytes: &bytes,
            range: self.range,
            symbol: self.symbol.as_deref(),
            base: self.base.as_deref(),
            budget: self.budget,
        }) {
            Some(report) => ToolOutput::report(report),
            None => unavailable(self.view.as_str()),
        }
    }
}

/// Controlo em falta quando a leitura (ou a view) não pode ser servida.
fn unavailable(control: &'static str) -> ToolOutput {
    let _span = katu_core::trace_fn!("read::unavailable");

    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

/// Ficheiro inexistente: relatório legível (evita o modelo repetir a leitura).
fn missing(path: &str) -> ToolOutput {
    let _span = katu_core::trace_fn!("read::missing");

    let data = Value::map(vec![
        ("path".to_string(), Value::str(path)),
        ("reason".to_string(), Value::str("not_found")),
    ]);
    ToolOutput::report(ToolReport::new("read.missing", data).with_next(vec![
        "verifique o caminho com `search` (mode=find)".to_string(),
    ]))
}
