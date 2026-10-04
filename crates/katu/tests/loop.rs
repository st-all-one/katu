//! E04-T08 — o **loop completo** com `FakeMemory`: `Task → … → Closed` pelo caminho real.
//!
//! Um *provider fake* é aqui um **guião determinístico** (a sequência de eventos que o modelo
//! emitiria); a memória é a porta real (`FakeMemory`). Prova que recall → write → close fecham o
//! caminho único com o gate de E05 e o log verificável. (O trait de provider é E12-T05.)

use katu_core::kernel::Visibility;
use katu_core::kernel::{
    CallContext, CallId, Event, MemoryWriteRequest, Message, Session, memory_recall_use,
};
use katu_core::memory::{
    Basis, FakeMemory, NoteRef, NoteType, PreWriteReq, RecallHit, RecallReq, Score,
};
use katu_core::plan::{Feature, FeatureStatus, Plan, ScopeContract};
use katu_core::ports::MemFs;
use katu_core::verify::{CheckStatus, VERIFICATION_SCHEMA_VERSION, VerificationReport};
use katu_policy::{Phase, ResolvedPath, RuleSet, ToolName};
use katu_tools::recall::RecallTool;
use katu_tools::write::WriteNoteTool;
use std::path::Path;

/// Regras reais do protocolo, versionadas no repositório.
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn rules() -> TestResult<RuleSet> {
    Ok(RuleSet::from_toml(MEMORY_POLICY)?)
}

fn cwd() -> TestResult<ResolvedPath> {
    Ok(ResolvedPath::from_canonical("/work")?)
}

fn plan() -> Plan {
    Plan::new(
        ScopeContract::new(
            Vec::new(),
            vec!["**/secrets/**".to_string()],
            Vec::new(),
            "reverter",
        ),
        vec![Feature::new(
            "F1",
            "gravar a decisão",
            FeatureStatus::Pending,
        )],
    )
}

fn verification_report() -> VerificationReport {
    VerificationReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        checks: Vec::new(),
        status: CheckStatus::Pass,
        coverage_bps: 10_000,
        strict: false,
    }
}

fn request() -> PreWriteReq {
    PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Decision,
        anchor: None,
        body: String::new(),
    }
}

fn hit() -> TestResult<RecallHit> {
    Ok(RecallHit {
        note: NoteRef::new("n1"),
        statement: "cache usa LRU".to_string(),
        score: Score::from_basis_points(9_000).ok_or("score inválido")?,
        basis: Basis::Measured,
        anchor: None,
    })
}

/// Avança uma fase do caminho único (sem evidência de fecho).
fn advance(session: &mut Session<'_>, to: Phase) -> TestResult<()> {
    session.apply(&Event::PhaseTransition { to, outcome: None })?;
    Ok(())
}

/// Recall explícito (tool `memory` search): marca `memory_recall` em `completed_tools`.
fn recall(
    session: &mut Session<'_>,
    memory: &FakeMemory,
    rules: &RuleSet,
    cwd: &ResolvedPath,
) -> TestResult<()> {
    let tool = RecallTool {
        memory,
        req: RecallReq {
            query: "cache".to_string(),
            limit: 5,
        },
    };
    let dispatch = session.tool_call(
        CallId::new("r1"),
        &memory_recall_use(cwd),
        CallContext {
            rules,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    assert_eq!(
        dispatch.report().map(|report| report.kind),
        Some("memory.recall")
    );
    Ok(())
}

/// Grava a nota pelo gate de E05 (o recall prévio autoriza).
fn write_note(
    session: &mut Session<'_>,
    memory: &FakeMemory,
    rules: &RuleSet,
    cwd: &ResolvedPath,
) -> TestResult<()> {
    let req = request();
    let tool = WriteNoteTool {
        memory,
        req: req.clone(),
    };
    let dispatch = session.memory_write(
        CallId::new("w1"),
        MemoryWriteRequest {
            cwd,
            req: &req,
            memory,
            rules,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    assert_eq!(
        dispatch.report().map(|report| report.kind),
        Some("memory.record")
    );
    Ok(())
}

#[test]
fn loop_runs_from_task_to_closed_with_fake_memory() -> TestResult<()> {
    let fs = MemFs::new();
    let memory = FakeMemory::with_hits(vec![hit()?]);
    let rules = rules()?;
    let cwd = cwd()?;
    let mut session = Session::open(&fs, Path::new("/sessions"))?;

    // O "provider fake" abre o turno e fala.
    session.apply(&Event::TurnStart { turn: 1 })?;
    session.apply(&Event::UserMessage {
        text: "guarda a decisão da cache".to_string(),
        visibility: Visibility::User,
    })?;

    recall(&mut session, &memory, &rules, &cwd)?;
    assert!(
        session
            .state()
            .completed_tools
            .contains(&ToolName::MemoryRecall)
    );
    advance(&mut session, Phase::KnowledgeConsulted)?;
    session.apply(&Event::PlanRecorded { plan: plan() })?;
    advance(&mut session, Phase::Planned)?;
    advance(&mut session, Phase::Implemented)?;
    session.record_verification(&verification_report())?;
    advance(&mut session, Phase::Verified)?;

    write_note(&mut session, &memory, &rules, &cwd)?;
    assert_eq!(memory.recorded(), 1);
    advance(&mut session, Phase::Persisted)?;
    session.apply(&Event::PhaseTransition {
        to: Phase::Closed,
        outcome: Some("nota gravada e verificada".to_string()),
    })?;
    session.apply(&Event::TurnEnd { turn: 1 })?;

    assert_eq!(session.state().phase, Phase::Closed);
    session.verify()?;
    let recall_calls = session
        .messages()?
        .iter()
        .filter(|message| {
            matches!(
                message,
                Message::ToolCall { tool, .. } if tool.name == ToolName::MemoryRecall
            )
        })
        .count();
    assert_eq!(recall_calls, 1);
    Ok(())
}

#[test]
fn loop_write_before_recall_is_a_wall_without_effect() -> TestResult<()> {
    let fs = MemFs::new();
    let memory = FakeMemory::default();
    let rules = rules()?;
    let cwd = cwd()?;
    let mut session = Session::open(&fs, Path::new("/sessions"))?;
    session.apply(&Event::TurnStart { turn: 1 })?;

    let req = request();
    let tool = WriteNoteTool {
        memory: &memory,
        req: req.clone(),
    };
    let dispatch = session.memory_write(
        CallId::new("w1"),
        MemoryWriteRequest {
            cwd: &cwd,
            req: &req,
            memory: &memory,
            rules: &rules,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    assert!(!dispatch.ran(), "sem recall não há escrita");
    assert_eq!(memory.recorded(), 0);
    session.verify()?;
    Ok(())
}
