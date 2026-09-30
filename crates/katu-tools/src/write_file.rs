//! Tool `write` de **ficheiro novo** (E06-T03/DF12): escrita atómica; existentes passam por
//! `edit`. Não decide política — só corre depois de `evaluate` permitir (ordem §42).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::Fs;
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolArgs, ToolName, ToolUse};

/// Executor de escrita de um ficheiro novo.
pub struct WriteFileTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Conteúdo a escrever.
    pub content: Vec<u8>,
}

impl Tool for WriteFileTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Write
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_WRITE);
        let ToolArgs::Write { path, .. } = &use_.args else {
            return unavailable("write");
        };
        let target = Path::new(path.as_str());
        if self.fs.exists(target) {
            return unavailable("exists");
        }
        if self.fs.write_atomic(target, &self.content).is_err() {
            return unavailable("write");
        }
        let id = content_id("f", path.as_str().as_bytes());
        let hash = content_hash(&self.content);
        let next = vec![format!("read {id}")];
        let bytes = u64::try_from(self.content.len()).unwrap_or(u64::MAX);
        let data = Value::map(vec![
            ("path".to_string(), Value::str(path.as_str())),
            (
                "bytes".to_string(),
                Value::int(i64::try_from(bytes).unwrap_or(i64::MAX)),
            ),
        ]);
        ToolOutput::report(
            ToolReport::new("write.file", data)
                .with_id(id)
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
    use super::WriteFileTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::Tool;
    use katu_core::ports::{Fs, MemFs};
    use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

    const PATH: &str = "/work/src/new.rs";

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical(PATH)?;
        Ok(ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: path.clone(),
                bytes: 3,
            },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    #[test]
    fn writes_new_file_and_reports() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let tool = WriteFileTool {
            fs: &fs,
            content: b"abc".to_vec(),
        };
        let output = tool.execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        let rendered = output
            .report
            .map_or_else(String::new, |report| report.to_toon());
        assert!(rendered.contains("write.file\u{1f}"), "{rendered}");
        assert!(rendered.contains("bytes\u{1f}3\n"), "{rendered}");
        assert_eq!(fs.read(std::path::Path::new(PATH))?, b"abc".to_vec());
        Ok(())
    }

    #[test]
    fn existing_file_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(std::path::Path::new(PATH), b"old")?;
        let tool = WriteFileTool {
            fs: &fs,
            content: b"abc".to_vec(),
        };
        assert!(matches!(
            tool.execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        assert_eq!(fs.read(std::path::Path::new(PATH))?, b"old".to_vec());
        Ok(())
    }
}
