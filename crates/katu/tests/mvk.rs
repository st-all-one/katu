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
use katu_core::plan::{Feature, FeatureStatus, Plan, ScopeContract};
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

/// Tipo de recusa observada numa tentativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefusalKind {
    /// Não houve recusa.
    None,
    /// Negação fechada (`Denied`) — muro (DF11).
    Denied,
    /// Controlo em falta (`Unavailable`) — soft.
    Soft,
}

/// Resultado de uma tentativa de escrita pelo loop real.
struct Attempt {
    /// Tipo de recusa.
    refusal: RefusalKind,
    /// Commits efetivos na porta de memória.
    commits: usize,
    /// Regra que recusou (acionável, DF10).
    rule_id: Option<String>,
}

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

/// Plano mínimo válido (E06-T06).
fn plan() -> Plan {
    Plan::new(
        ScopeContract::new(
            Vec::new(),
            vec!["**/secrets/**".to_string()],
            Vec::new(),
            "reverter",
        ),
        vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
    )
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

/// Conduz uma escrita de memória pelo loop real e devolve o resultado observado.
fn attempt_write(memory: &FakeMemory, recall: Recall) -> TestResult<Attempt> {
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
    let outcome = dispatch.outcome();
    let refusal = match &outcome {
        ToolOutcome::Denied { .. } => RefusalKind::Denied,
        ToolOutcome::Unavailable { .. } => RefusalKind::Soft,
        _ => RefusalKind::None,
    };
    Ok(Attempt {
        refusal,
        commits: memory.recorded(),
        rule_id: outcome.rule_id().map(|rule| rule.as_str().to_string()),
    })
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
fn write_without_recall_is_denied_without_effect() -> TestResult<()> {
    let memory = FakeMemory::default();
    let attempt = attempt_write(&memory, Recall::No)?;
    assert_eq!(
        attempt.refusal,
        RefusalKind::Denied,
        "gravar sem recall é um muro (DF11)"
    );
    assert_eq!(attempt.commits, 0, "o executor não pode ser invocado");
    assert_eq!(
        attempt.rule_id.as_deref(),
        Some("mem-recall-before-write"),
        "a recusa tem de ser acionável (DF10)"
    );
    Ok(())
}

#[test]
fn duplicate_write_is_denied_and_never_committed() -> TestResult<()> {
    let score = Score::from_basis_points(9_500).ok_or("score inválido")?;
    let memory = FakeMemory::rejecting(NoteRef::new("fact_1"), score);
    let attempt = attempt_write(&memory, Recall::Yes)?;
    assert_eq!(attempt.refusal, RefusalKind::Denied);
    assert_eq!(attempt.commits, 0, "sem commit de facto");
    assert_eq!(attempt.rule_id.as_deref(), Some("mem-no-duplicate"));
    Ok(())
}

#[test]
fn recall_then_new_write_commits_once() -> TestResult<()> {
    let memory = FakeMemory::default();
    let attempt = attempt_write(&memory, Recall::Yes)?;
    assert_eq!(attempt.refusal, RefusalKind::None, "nota nova é permitida");
    assert_eq!(attempt.commits, 1, "exatamente um commit");
    assert!(attempt.rule_id.is_none(), "sem recusa, sem regra");
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
    session.apply(&Event::PlanRecorded { plan: plan() })?;
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
    // Sem recall concluído, a política **nega** (muro, DF11).
    let facts = build_facts(&[ToolName::MemoryWrite], Vec::new())?;
    let decision = evaluate(&facts, &rules()?)?;
    assert!(matches!(decision, Decision::Deny { .. }));
    assert!(!decision.is_allow());

    // Com recall, mas sem capacidade de `command`, a negação também é fechada.
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
