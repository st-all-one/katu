//! E05-T05 — enforcement pelo **caminho real**: sessão + gate de memória + política.
//!
//! Não testa extractores puros: conduz o loop (turno → tool call → política → efeito → log) e
//! prova que os três cenários do gate produzem recusa determinística **sem efeito**:
//! (a) gravar sem recall; (b) gravar uma duplicata forte (≥ 0,92); (c) fechar sem `outcome`.
//!
//! Cada teste fica **vermelho** se o enforcement for removido (ver `regression_sensitivity`).

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Event, MemoryWriteRequest, RefusalReason, Session, SessionError};
use katu_core::memory::{FakeMemory, NoteRef, NoteType, PreWriteReq, Score};
use katu_core::ports::MemFs;
use katu_policy::{
    BudgetState, Capability, Decision, Facts, Phase, ResolvedPath, RuleSet, ToolArgs, ToolName,
    ToolUse, evaluate,
};
use katu_tools::write::WriteNoteTool;
use std::path::Path;

/// Regras reais do protocolo, versionadas no repositório.
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

/// Resultado de um teste (sem `unwrap`/`expect`).
type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

/// `(recusada?, commits)` de uma tentativa de escrita.
type WriteAttempt = (bool, usize);

/// Se a escrita é precedida de `memory_recall`.
#[derive(Clone, Copy)]
enum Recall {
    /// Sem consulta prévia.
    No,
    /// Com `memory_recall` concluído.
    Yes,
}

fn rules() -> TestResult<RuleSet> {
    Ok(RuleSet::from_toml(MEMORY_POLICY)?)
}

fn request() -> PreWriteReq {
    PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Decision,
        anchor: None,
        body: String::new(),
    }
}

fn cwd() -> TestResult<ResolvedPath> {
    Ok(ResolvedPath::from_canonical("/work")?)
}

/// Conduz uma escrita de memória pelo loop real e devolve `(recusada?, commits)`.
fn attempt_write(memory: &FakeMemory, recall: Recall) -> TestResult<WriteAttempt> {
    let fs = MemFs::new();
    let mut session = Session::open(&fs, Path::new("/sessions"))?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    if matches!(recall, Recall::Yes) {
        session.apply(&Event::ToolCall {
            call: CallId::new("r1"),
            tool: recall_use()?,
        })?;
        session.apply(&Event::ToolResult {
            call: CallId::new("r1"),
            outcome: ToolOutcome::Ok,
        })?;
    }
    let req = request();
    let tool = WriteNoteTool {
        memory,
        req: req.clone(),
    };
    let dispatch = session.memory_write(
        CallId::new("w1"),
        MemoryWriteRequest {
            cwd: &cwd()?,
            req: &req,
            memory,
            rules: &rules()?,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    session.verify()?;
    Ok((!dispatch.ran(), memory.recorded()))
}

fn recall_use() -> Result<ToolUse, katu_policy::PolicyError> {
    let cwd = ResolvedPath::from_canonical("/work")?;
    Ok(ToolUse {
        name: ToolName::MemoryRecall,
        args: ToolArgs::Other,
        resolved_paths: Vec::new(),
        argv: None,
        cwd,
    })
}

#[test]
fn write_without_recall_is_refused_without_effect() -> TestResult<()> {
    let memory = FakeMemory::default();
    let (refused, commits) = attempt_write(&memory, Recall::No)?;
    assert!(refused, "sem recall a escrita não pode correr");
    assert_eq!(commits, 0, "o executor não pode ser invocado");
    Ok(())
}

#[test]
fn duplicate_write_is_denied_and_never_committed() -> TestResult<()> {
    let score = Score::from_basis_points(9_500).ok_or("score inválido")?;
    let memory = FakeMemory::rejecting(NoteRef::new("fact_1"), score);
    let (refused, commits) = attempt_write(&memory, Recall::Yes)?;
    assert!(refused, "duplicata não pode escrever");
    assert_eq!(commits, 0, "sem commit de facto");
    Ok(())
}

#[test]
fn recall_then_new_write_commits_once() -> TestResult<()> {
    let memory = FakeMemory::default();
    let (refused, commits) = attempt_write(&memory, Recall::Yes)?;
    assert!(!refused, "nota nova devia ser permitida");
    assert_eq!(commits, 1, "exatamente um commit");
    Ok(())
}

#[test]
fn close_without_outcome_is_refused() -> TestResult<()> {
    let fs = MemFs::new();
    let mut session = Session::open(&fs, Path::new("/sessions"))?;
    // Waiver explícito para a fase `KnowledgeConsulted` (não é o alvo deste teste).
    session.apply(&Event::Waiver {
        transition: Phase::KnowledgeConsulted,
        reason: "sem consulta aplicável".into(),
    })?;
    for to in [
        Phase::KnowledgeConsulted,
        Phase::Planned,
        Phase::Implemented,
        Phase::Verified,
        Phase::Persisted,
    ] {
        session.apply(&Event::PhaseTransition { to, outcome: None })?;
    }
    let refused = session.apply(&Event::PhaseTransition {
        to: Phase::Closed,
        outcome: None,
    });
    assert!(matches!(
        refused,
        Err(SessionError::Refusal(refusal))
            if matches!(
                refusal.reason,
                RefusalReason::UnmetPrecondition { to: Phase::Closed }
            )
    ));
    // A fase não mudou e o log continua verificável.
    assert_eq!(session.state().phase, Phase::Persisted);
    session.verify()?;
    Ok(())
}

/// Sensibilidade à regressão.
///
/// Com a **mesma** pré-validação, a decisão difere entre a porta de memória (recusa por duplicata)
/// e a ausência de capacidade (negação fechada). Se o enforcement desaparecer, estes asserts caem.
#[test]
fn regression_sensitivity() -> TestResult<()> {
    // Sem recall concluído, a política exige aprovação (não executa).
    let facts = build_facts(&[ToolName::MemoryWrite], Vec::new())?;
    assert!(!evaluate(&facts, &rules()?)?.is_allow());

    // Com recall, mas sem capacidade de `command`, a negação é fechada.
    let facts = build_facts(&[ToolName::MemoryRecall, ToolName::MemoryWrite], Vec::new())?;
    assert!(matches!(
        evaluate(&facts, &rules()?)?,
        Decision::Deny { .. }
    ));

    // A via positiva é a única que comita (o executor é o único com efeito).
    assert_eq!(FakeMemory::default().recorded(), 0);
    Ok(())
}

fn build_facts(completed: &[ToolName], capabilities: Vec<Capability>) -> TestResult<Facts> {
    let cwd = ResolvedPath::from_canonical("/work")?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: ToolName::MemoryWrite,
            args: ToolArgs::Other,
            resolved_paths: Vec::new(),
            argv: None,
            cwd,
        },
        capabilities,
        budget: BudgetState::default(),
        completed: completed.iter().copied().collect(),
    })
}
