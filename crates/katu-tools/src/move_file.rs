//! Tool `move` (E06-T11): renomeação **atómica** sob escopo, sem sobrescrever o destino.
//!
//! `rename` não altera o conteúdo; devolvem-se o `id`/`hash` do **novo** caminho para o modelo não
//! precisar de reler. O destino existente é recusado (fail-closed: nunca há *clobber* silencioso).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::Fs;
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

/// Executor de movimento/renomeação atómica.
pub struct MoveFileTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
}

impl Tool for MoveFileTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Move
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_MOVE);
        let ToolArgs::Move { from, to } = &use_.args else {
            return unavailable("move");
        };
        let source = Path::new(from.as_str());
        let target = Path::new(to.as_str());
        let Ok(bytes) = self.fs.read(source) else {
            return unavailable("missing");
        };
        if self.fs.exists(target) {
            return unavailable("exists");
        }
        if self.fs.rename(source, target).is_err() {
            return unavailable("write");
        }
        let from_id = content_id("f", from.as_str().as_bytes());
        let to_id = content_id("f", to.as_str().as_bytes());
        let hash = content_hash(&bytes);
        let count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let data = Value::map(vec![
            ("from".to_string(), Value::str(from.as_str())),
            ("to".to_string(), Value::str(to.as_str())),
            ("from_id".to_string(), Value::str(from_id)),
            ("to_id".to_string(), Value::str(to_id.clone())),
            (
                "bytes".to_string(),
                Value::int(i64::try_from(count).unwrap_or(i64::MAX)),
            ),
        ]);
        let next = vec![format!("read {to_id}")];
        ToolOutput::report(
            ToolReport::new("move.file", data)
                .with_id(to_id)
                .with_hash(hash)
                .with_next(next),
        )
    }
}

fn unavailable(control: &'static str) -> ToolOutput {
    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::MoveFileTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::Tool;
    use katu_core::ports::{Fs, MemFs};
    use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
    use std::path::Path;

    const FROM: &str = "/work/src/old.rs";
    const TO: &str = "/work/src/new.rs";

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        let from = ResolvedPath::from_canonical(FROM)?;
        let to = ResolvedPath::from_canonical(TO)?;
        Ok(ToolUse {
            name: ToolName::Move,
            args: ToolArgs::Move {
                from: from.clone(),
                to: to.clone(),
            },
            resolved_paths: vec![from.clone(), to],
            argv: None,
            cwd: from,
        })
    }

    #[test]
    fn moves_and_reports_new_id() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new(FROM), b"pub fn a() {}\n")?;
        let tool = MoveFileTool { fs: &fs };
        let output = tool.execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        assert!(!fs.exists(Path::new(FROM)));
        assert!(fs.exists(Path::new(TO)));
        assert_eq!(fs.read(Path::new(TO))?, b"pub fn a() {}\n".to_vec());
        let rendered = output
            .report
            .map_or_else(String::new, |report| report.to_toon());
        assert!(rendered.contains("move.file\u{1f}"), "{rendered}");
        assert!(rendered.contains("from\u{1f}"), "{rendered}");
        assert!(rendered.contains("to_id\u{1f}f_"), "{rendered}");
        Ok(())
    }

    #[test]
    fn refuses_to_overwrite_the_destination() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new(FROM), b"old")?;
        fs.write_atomic(Path::new(TO), b"keep")?;
        let tool = MoveFileTool { fs: &fs };
        assert!(matches!(
            tool.execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        assert_eq!(fs.read(Path::new(FROM))?, b"old".to_vec());
        assert_eq!(fs.read(Path::new(TO))?, b"keep".to_vec());
        Ok(())
    }

    #[test]
    fn missing_source_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let tool = MoveFileTool { fs: &fs };
        assert!(matches!(
            tool.execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        assert!(!fs.exists(Path::new(TO)));
        Ok(())
    }
}
