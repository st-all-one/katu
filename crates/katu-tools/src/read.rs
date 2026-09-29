//! Tool `read` mínima (E05-T02): fonte de verdade da fase `KnowledgeConsulted`.
//!
//! Lê um ficheiro já resolvido pela política (o `ToolUse` carrega `ResolvedPath`); não decide
//! política nem confina além do que a canonicalização garante (a jail real é E17).

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::Tool;
use katu_core::ports::Fs;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};
use std::path::Path;

/// Executor de uma leitura.
pub struct ReadTool<'a> {
    /// Porta de sistema de ficheiros.
    pub fs: &'a dyn Fs,
}

impl Tool for ReadTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Read
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutcome {
        let _span = katu_core::span!(Level::Trace, events::TOOL_READ);
        let ToolArgs::Read { path } = &use_.args else {
            return unavailable();
        };
        match self.fs.read(Path::new(path.as_str())) {
            Ok(_) => ToolOutcome::Ok,
            Err(_) => unavailable(),
        }
    }
}

/// Controlo em falta quando a leitura não pode ser feita.
fn unavailable() -> ToolOutcome {
    ToolOutcome::Unavailable {
        control: ControlId::new("read"),
        rule_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::ReadTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::Tool;
    use katu_core::ports::{Fs, MemFs};
    use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical("/work/src/lib.rs")?;
        Ok(ToolUse {
            name: ToolName::Read,
            args: ToolArgs::Read { path: path.clone() },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    #[test]
    fn reads_existing_file() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(std::path::Path::new("/work/src/lib.rs"), b"pub fn x() {}")?;
        let tool = ReadTool { fs: &fs };
        assert_eq!(tool.execute(&use_()?), ToolOutcome::Ok);
        Ok(())
    }

    #[test]
    fn missing_file_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let tool = ReadTool { fs: &fs };
        assert!(matches!(
            tool.execute(&use_()?),
            ToolOutcome::Unavailable { .. }
        ));
        Ok(())
    }
}
