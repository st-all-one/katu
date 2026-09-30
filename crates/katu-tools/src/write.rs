//! Tool `write` de nota (E05-T01): o executor do commit de memória.
//!
//! Não decide política: o kernel só a invoca **depois** de `evaluate` permitir (ordem §42), pelo que
//! uma negação nunca tem efeito. A porta `Memory` faz o commit por nota (OA8).

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::memory::{Memory, NoteRef, PreWriteReq};
use katu_core::report::ToolReport;
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolName, ToolUse};

/// Executor do commit de uma nota pré-validada.
pub struct WriteNoteTool<'a> {
    /// Porta de memória (o commit vive aqui).
    pub memory: &'a dyn Memory,
    /// Nota pré-validada (a mesma que passou o `pre_write`).
    pub req: PreWriteReq,
}

impl Tool for WriteNoteTool<'_> {
    fn name(&self) -> ToolName {
        let _span = katu_core::trace_fn!("write::name");

        ToolName::MemoryWrite
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_WRITE, "write::execute");
        match self.memory.record(&self.req) {
            Ok(note) => ToolOutput::report(record_report(&note)),
            Err(err) if err.retryable() => ToolOutput::outcome(ToolOutcome::Timeout),
            Err(_) => ToolOutput::outcome(ToolOutcome::Unavailable {
                control: ControlId::new("memory"),
                rule_id: None,
            }),
        }
    }
}

/// Envelope AI-first do commit de memória (DF12).
fn record_report(note: &NoteRef) -> ToolReport {
    let _span = katu_core::trace_fn!("write::record_report");

    let data = Value::map(vec![(
        "note".to_string(),
        Value::str(note.as_str().to_string()),
    )]);
    ToolReport::new("memory.record", data).with_id(note.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::WriteNoteTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::{Tool, memory_write_use};
    use katu_core::memory::{FakeMemory, MemoryErrorKind, NoteType, PreWriteReq};
    use katu_policy::{ResolvedPath, ToolUse};

    fn request() -> PreWriteReq {
        PreWriteReq {
            statement: "cache usa LRU".to_string(),
            note_type: NoteType::Decision,
            anchor: None,
            body: String::new(),
        }
    }

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        Ok(memory_write_use(&ResolvedPath::from_canonical("/work")?))
    }

    #[test]
    fn write_note_records_once() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::default();
        let tool = WriteNoteTool {
            memory: &memory,
            req: request(),
        };
        let output = tool.execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        assert_eq!(memory.recorded(), 1);
        let report = output.report.as_ref().ok_or("sem envelope")?;
        assert_eq!(report.kind, "memory.record");
        assert!(report.to_toon().contains("memory.record"), "{report:?}");
        Ok(())
    }

    #[test]
    fn write_note_reports_unavailable_on_failure() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::failing(MemoryErrorKind::Internal);
        let tool = WriteNoteTool {
            memory: &memory,
            req: request(),
        };
        assert!(matches!(
            tool.execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        assert_eq!(memory.recorded(), 0);
        Ok(())
    }
}
