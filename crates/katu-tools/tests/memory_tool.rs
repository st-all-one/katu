//! E06-T10 — tool `memory`: pedido explícito, no **grupo de controlo**, com o gate no kernel.
//!
//! A tool só **pede**; quem valida (dedup/âncora/`outcome`) é o gate `pre_write` → capacidade →
//! política (E05). Registar ou não a tool **não** altera o enforcement.

use katu_core::kernel::{MemoryWriteRequest, State, enforce_memory_write};
use katu_core::memory::{FakeMemory, NoteRef, NoteType, PreWriteReq, Score};
use katu_policy::{ResolvedPath, RuleSet, ToolName};
use katu_tools::registry::{Family, TOOLS, ToolId, is_registered};
use katu_tools::write::WriteNoteTool;

const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn request() -> PreWriteReq {
    PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Decision,
        anchor: None,
        body: String::new(),
    }
}

fn state_with_recall() -> State {
    let mut state = State::initial();
    state.completed_tools.insert(ToolName::MemoryRecall);
    state
}

#[test]
fn memory_tool_is_in_the_control_family() -> TestResult<()> {
    let spec = TOOLS
        .iter()
        .find(|spec| spec.id == ToolId::Memory)
        .ok_or("tool `memory` não registada")?;
    assert!(spec.names.contains(&ToolName::MemoryWrite));
    assert!(spec.names.contains(&ToolName::MemoryRecall));
    assert_eq!(spec.family, Family::Control);
    assert!(is_registered(ToolName::MemoryWrite));
    assert!(is_registered(ToolName::MemoryRecall));
    Ok(())
}

#[test]
fn allowed_write_returns_a_memory_record_report() -> TestResult<()> {
    let memory = FakeMemory::default();
    let req = request();
    let tool = WriteNoteTool {
        memory: &memory,
        req: req.clone(),
    };
    let rules = RuleSet::from_toml(MEMORY_POLICY)?;
    let cwd = ResolvedPath::from_canonical("/work")?;
    let dispatch = enforce_memory_write(
        &state_with_recall(),
        MemoryWriteRequest {
            cwd: &cwd,
            req: &req,
            memory: &memory,
            rules: &rules,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    assert!(dispatch.ran());
    assert_eq!(
        dispatch.report().map(|report| report.kind),
        Some("memory.record")
    );
    assert_eq!(memory.recorded(), 1);
    Ok(())
}

#[test]
fn duplicate_write_is_denied_even_with_the_tool_registered() -> TestResult<()> {
    let score = Score::from_basis_points(9_500).ok_or("score inválido")?;
    let memory = FakeMemory::rejecting(NoteRef::new("fact_1"), score);
    let req = request();
    let tool = WriteNoteTool {
        memory: &memory,
        req: req.clone(),
    };
    let rules = RuleSet::from_toml(MEMORY_POLICY)?;
    let cwd = ResolvedPath::from_canonical("/work")?;
    let dispatch = enforce_memory_write(
        &state_with_recall(),
        MemoryWriteRequest {
            cwd: &cwd,
            req: &req,
            memory: &memory,
            rules: &rules,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    assert!(!dispatch.ran(), "a tool não contorna o `pre_write`");
    assert_eq!(memory.recorded(), 0);
    Ok(())
}
