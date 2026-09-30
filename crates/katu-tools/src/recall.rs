//! Tool `memory` (recall) — E06-T10: consulta explícita, policy-gated, secundária aos hooks.
//!
//! Só **pede**: quem responde é a porta `Memory`. O resultado entra no log e marca
//! `memory_recall` em `completed_tools` (pré-condição de `memory_write`,
//! `mem-recall-before-write`).

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::memory::{Memory, RecallHit, RecallReq};
use katu_core::report::{Page, ToolReport};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolName, ToolUse};

use crate::lang::{len_u64, to_i64};

/// Executor do recall de memória.
pub struct RecallTool<'a> {
    /// Porta de memória (a busca vive aqui).
    pub memory: &'a dyn Memory,
    /// Consulta.
    pub req: RecallReq,
}

impl Tool for RecallTool<'_> {
    fn name(&self) -> ToolName {
        ToolName::MemoryRecall
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::span!(Level::Trace, events::MEMORY_RECALL);
        match self.memory.search(&self.req) {
            Ok(hits) => ToolOutput::report(recall_report(&self.req.query, &hits)),
            Err(err) if err.retryable() => ToolOutput::outcome(ToolOutcome::Timeout),
            Err(_) => ToolOutput::outcome(ToolOutcome::Unavailable {
                control: ControlId::new("memory"),
                rule_id: None,
            }),
        }
    }
}

/// Envelope AI-first do recall (DF12): só o delta (nota + afirmação + score) chega ao modelo.
fn recall_report(query: &str, hits: &[RecallHit]) -> ToolReport {
    let items = hits
        .iter()
        .enumerate()
        .map(|(index, hit)| hit_value(hit, index))
        .collect();
    let data = Value::map(vec![
        ("query".to_string(), Value::str(query.to_string())),
        ("hits".to_string(), Value::list(items)),
    ]);
    ToolReport::new("memory.recall", data)
        .with_id(format!("recall:{}", hits.len()))
        .with_page(Page::complete(
            u64::try_from(hits.len()).unwrap_or(u64::MAX),
        ))
}

/// Uma nota recordada como `Value` TOON (com `rank` e evidência `ev`).
fn hit_value(hit: &RecallHit, rank: usize) -> Value {
    let mut entries = vec![
        (
            "rank".to_string(),
            Value::int(to_i64(len_u64(rank.saturating_add(1)))),
        ),
        (
            "note".to_string(),
            Value::str(hit.note.as_str().to_string()),
        ),
        ("statement".to_string(), Value::str(hit.statement.clone())),
        (
            "score".to_string(),
            Value::int(i64::from(hit.score.as_basis_points().saturating_div(10))),
        ),
        ("basis".to_string(), Value::str(hit.basis.as_str())),
    ];
    if let Some(anchor) = &hit.anchor {
        entries.push(("ev".to_string(), Value::str(anchor.as_str().to_string())));
    }
    Value::map(entries)
}

#[cfg(test)]
mod tests {
    use super::RecallTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::Tool;
    use katu_core::memory::{
        Basis, FakeMemory, MemoryErrorKind, NoteRef, RecallHit, RecallReq, Score,
    };
    use katu_policy::{ResolvedPath, ToolName, ToolUse};

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        Ok(ToolUse {
            name: ToolName::MemoryRecall,
            args: katu_policy::ToolArgs::Other,
            resolved_paths: Vec::new(),
            argv: None,
            cwd: ResolvedPath::from_canonical("/work")?,
        })
    }

    fn hit() -> Result<RecallHit, Box<dyn std::error::Error>> {
        Ok(RecallHit {
            note: NoteRef::new("n1"),
            statement: "cache usa LRU".to_string(),
            score: Score::from_basis_points(9_000).ok_or("score inválido")?,
            basis: Basis::Measured,
            anchor: None,
        })
    }

    #[test]
    fn recall_returns_a_memory_recall_report() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::with_hits(vec![hit()?]);
        let tool = RecallTool {
            memory: &memory,
            req: RecallReq {
                query: "cache".to_string(),
                limit: 5,
            },
        };
        let output = tool.execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        let report = output.report.as_ref().ok_or("sem envelope")?;
        assert_eq!(report.kind, "memory.recall");
        assert!(report.to_toon().contains("memory.recall"), "{report:?}");
        assert!(report.to_toon().contains("n1"), "{report:?}");
        Ok(())
    }

    #[test]
    fn recall_reports_unavailable_on_failure() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::failing(MemoryErrorKind::Internal);
        let tool = RecallTool {
            memory: &memory,
            req: RecallReq {
                query: "cache".to_string(),
                limit: 5,
            },
        };
        assert!(matches!(
            tool.execute(&use_()?).outcome,
            ToolOutcome::Unavailable { .. }
        ));
        Ok(())
    }

    #[test]
    fn recall_times_out_on_retryable_failure() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::failing(MemoryErrorKind::Timeout);
        let tool = RecallTool {
            memory: &memory,
            req: RecallReq {
                query: "cache".to_string(),
                limit: 5,
            },
        };
        assert_eq!(tool.execute(&use_()?).outcome, ToolOutcome::Timeout);
        Ok(())
    }
}
