//! Tool `edit` **otimista** (E06-T03/OA16): patch `old`→`new` com *compare-and-swap*.
//!
//! `read` → aplicar patch → `Fs::write_atomic_if` (só grava se o conteúdo atual casar com o lido).
//! Se o ficheiro mudou entretanto, devolve `Unavailable { control: "stale" }` — **recuperável**
//! ("relê e reaplica"), nunca sobrescreve edição concorrente. `dry_run` mostra o patch sem gravar.

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::ports::{Fs, FsError};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ResolvedPath, ToolArgs, ToolName, ToolUse};

/// Executor de um patch otimista.
pub struct EditFileTool<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Trecho a substituir (tem de ser **único**).
    pub old: String,
    /// Trecho novo.
    pub new: String,
    /// Se `true`, não grava (mostra o patch).
    pub dry_run: bool,
}

impl Tool for EditFileTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::Edit
    }

    fn execute(&self, use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::TOOL_EDIT);
        let ToolArgs::Edit { path } = &use_.args else {
            return unavailable("edit");
        };
        let Ok(current) = self.fs.read(Path::new(path.as_str())) else {
            return unavailable("read");
        };
        let text = String::from_utf8_lossy(&current);
        match text.matches(&self.old).count() {
            0 => return unavailable("patch"),
            1 => {}
            _ => return unavailable("ambiguous"),
        }
        let updated = text.replacen(&self.old, &self.new, 1);
        let id = content_id("f", path.as_str().as_bytes());
        let old_hash = content_hash(&current);
        let new_hash = content_hash(updated.as_bytes());
        if self.dry_run {
            return ToolOutput::report(report("edit.dry-run", path, &id, &old_hash, &new_hash));
        }
        match self
            .fs
            .write_atomic_if(Path::new(path.as_str()), updated.as_bytes(), &current)
        {
            Ok(()) => ToolOutput::report(report("edit.patch", path, &id, &old_hash, &new_hash)),
            Err(FsError::Stale) => unavailable("stale"),
            Err(_) => unavailable("write"),
        }
    }
}

fn report(
    kind: &'static str,
    path: &ResolvedPath,
    id: &str,
    old_hash: &str,
    new_hash: &str,
) -> ToolReport {
    let data = Value::map(vec![
        ("path".to_string(), Value::str(path.as_str())),
        ("hunks".to_string(), Value::int(1)),
        ("old_hash".to_string(), Value::str(old_hash)),
        ("new_hash".to_string(), Value::str(new_hash)),
        ("breaking".to_string(), Value::bool(false)),
    ]);
    ToolReport::new(kind, data).with_id(id).with_hash(new_hash)
}

fn unavailable(control: &'static str) -> ToolOutput {
    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::EditFileTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::Tool;
    use katu_core::ports::{Fs, MemFs};
    use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

    const PATH: &str = "/work/src/lib.rs";

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical(PATH)?;
        Ok(ToolUse {
            name: ToolName::Edit,
            args: ToolArgs::Edit { path: path.clone() },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "teste: `dry_run` é o eixo do cenário"
    )]
    fn tool(fs: &MemFs, dry_run: bool) -> EditFileTool<'_> {
        EditFileTool {
            fs,
            old: "let x = 1;".to_string(),
            new: "let x = 2;".to_string(),
            dry_run,
        }
    }

    #[test]
    fn patch_applies_and_reports_hashes() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(std::path::Path::new(PATH), b"fn a() { let x = 1; }\n")?;
        let output = tool(&fs, false).execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        let report = output.report.ok_or("sem relatório")?;
        let toon = report.to_toon();
        assert!(toon.contains("kind: edit.patch\n"), "{toon}");
        assert!(toon.contains("hunks: 1\n"), "{toon}");
        assert_eq!(
            fs.read(std::path::Path::new(PATH))?,
            b"fn a() { let x = 2; }\n"
        );
        Ok(())
    }

    #[test]
    fn dry_run_does_not_write() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let original = b"fn a() { let x = 1; }\n";
        fs.write_atomic(std::path::Path::new(PATH), original)?;
        let output = tool(&fs, true).execute(&use_()?);
        assert!(
            output
                .report
                .is_some_and(|report| report.to_toon().contains("edit.dry-run"))
        );
        assert_eq!(fs.read(std::path::Path::new(PATH))?, original.to_vec());
        Ok(())
    }

    #[test]
    fn ambiguous_patch_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(std::path::Path::new(PATH), b"let x = 1; let x = 1;\n")?;
        assert!(matches!(
            tool(&fs, false).execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        Ok(())
    }

    #[test]
    fn missing_patch_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(std::path::Path::new(PATH), b"nada aqui\n")?;
        assert!(matches!(
            tool(&fs, false).execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        Ok(())
    }
}
