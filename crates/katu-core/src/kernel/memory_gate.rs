//! Enforcement da escrita de memória (E05-T01): `pre_write` → capacidade → política → efeito.
//!
//! O adaptador de memória **concede** `Capability::Command { tool: MemoryWrite }` apenas quando o
//! `pre_write` não rejeita (sem duplicata forte; âncora/single-claim validados pelo backend). Sem a
//! capacidade, as regras `deny_command` do protocolo (E02-T07) disparam — falha fechada (DF4). O
//! executor (a tool `write` de nota, E03/E06) **só** corre depois de `evaluate` permitir, pelo que
//! uma negação nunca tem efeito (§42).

use super::pipeline::{Dispatch, DispatchRequest, Tool, dispatch_with};
use super::state::State;
use crate::diag::{Level, events};
use crate::memory::{Memory, MemoryError, PreWriteOutcome, PreWriteReq};
use katu_policy::{Capability, PolicyError, ResolvedPath, RuleSet, ToolArgs, ToolName, ToolUse};

/// Pedido de escrita de nota, com o que a política e o executor precisam.
#[derive(Clone, Copy)]
pub struct MemoryWriteRequest<'a> {
    /// Diretório de trabalho (contexto do `ToolUse`; a memória não é *path-based*).
    pub cwd: &'a ResolvedPath,
    /// Nota a pré-validar.
    pub req: &'a PreWriteReq,
    /// Porta de memória (para o `pre_write`).
    pub memory: &'a dyn Memory,
    /// Regras do protocolo (`policy/memory.toml`).
    pub rules: &'a RuleSet,
    /// Instante corrente (ms desde a época).
    pub now_millis: u64,
    /// Executor do commit (a tool de nota).
    pub tool: &'a dyn Tool,
}

/// Erro ao avaliar uma escrita de memória.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MemoryWriteError {
    /// Vocabulário de política inválido.
    #[error("política: {0}")]
    Policy(#[from] PolicyError),
    /// Falha da porta de memória.
    #[error("memória: {0}")]
    Memory(#[from] MemoryError),
}

/// Constrói o `ToolUse` resolvido de uma escrita de memória.
#[must_use]
pub fn memory_write_use(cwd: &ResolvedPath) -> ToolUse {
    let _span = crate::trace_fn!("kernel::memory_gate::memory_write_use");

    ToolUse {
        name: ToolName::MemoryWrite,
        args: ToolArgs::Other,
        resolved_paths: Vec::new(),
        argv: None,
        cwd: cwd.clone(),
    }
}

/// Constrói o `ToolUse` resolvido de um recall de memória (E06-T10).
#[must_use]
pub fn memory_recall_use(cwd: &ResolvedPath) -> ToolUse {
    let _span = crate::trace_fn!("kernel::memory_gate::memory_recall_use");

    ToolUse {
        name: ToolName::MemoryRecall,
        args: ToolArgs::Other,
        resolved_paths: Vec::new(),
        argv: None,
        cwd: cwd.clone(),
    }
}

/// Avalia e executa uma escrita de memória: `pre_write` → capacidade → política → efeito.
///
/// # Errors
/// [`MemoryWriteError`] se o `pre_write` falhar ou a política for inválida (fail-closed).
pub fn enforce_memory_write(
    state: &State,
    request: MemoryWriteRequest<'_>,
) -> Result<Dispatch, MemoryWriteError> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::MEMORY_WRITE,
        "kernel::memory_gate::enforce_memory_write"
    );
    let pre_write = request.memory.pre_write(request.req)?;
    let capabilities = capabilities_for(&pre_write);
    let use_ = memory_write_use(request.cwd);
    let dispatch = dispatch_with(DispatchRequest {
        state,
        use_: &use_,
        rules: request.rules,
        now_millis: request.now_millis,
        capabilities: &capabilities,
        tool: request.tool,
    })?;
    Ok(dispatch)
}

/// Capacidade concedida pela pré-validação: só grava quem não é rejeitado.
fn capabilities_for(outcome: &PreWriteOutcome) -> Vec<Capability> {
    let _span = crate::trace_fn!("kernel::memory_gate::capabilities_for");

    match outcome {
        PreWriteOutcome::Reject { .. } => Vec::new(),
        PreWriteOutcome::Create | PreWriteOutcome::Merge { .. } => vec![Capability::Command {
            tool: ToolName::MemoryWrite,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryWriteRequest, enforce_memory_write, memory_write_use};
    use crate::error::ToolOutcome;
    use crate::kernel::pipeline::{Tool, ToolOutput};
    use crate::kernel::state::State;
    use crate::memory::{FakeMemory, NoteRef, NoteType, PreWriteReq, Score};
    use katu_policy::{ResolvedPath, RuleId, RuleSet, ToolName, ToolUse};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Regras reais do protocolo, versionadas no repositório.
    const MEMORY_POLICY: &str = include_str!("../../../../policy/memory.toml");

    struct CommitProbe {
        calls: AtomicUsize,
    }

    impl Tool for CommitProbe {
        fn name(&self) -> ToolName {
            ToolName::MemoryWrite
        }

        fn execute(&self, _use_: &ToolUse) -> ToolOutput {
            self.calls.fetch_add(1, Ordering::SeqCst);
            ToolOutput::ok()
        }
    }

    fn rules() -> Result<RuleSet, Box<dyn std::error::Error>> {
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

    fn cwd() -> Result<ResolvedPath, Box<dyn std::error::Error>> {
        Ok(ResolvedPath::from_canonical("/work")?)
    }

    fn state_with_recall() -> State {
        let mut state = State::initial();
        state.completed_tools.insert(ToolName::MemoryRecall);
        state
    }

    #[test]
    fn duplicate_write_is_denied_with_rule_and_no_effect() -> Result<(), Box<dyn std::error::Error>>
    {
        let score = Score::from_basis_points(9_500).ok_or("score inválido")?;
        let memory = FakeMemory::rejecting(NoteRef::new("fact_1"), score);
        let probe = CommitProbe {
            calls: AtomicUsize::new(0),
        };
        let (req, rules, cwd) = (request(), rules()?, cwd()?);
        let dispatch = enforce_memory_write(
            &state_with_recall(),
            MemoryWriteRequest {
                cwd: &cwd,
                req: &req,
                memory: &memory,
                rules: &rules,
                now_millis: 0,
                tool: &probe,
            },
        )?;
        assert!(!dispatch.ran(), "nada pode correr");
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert_eq!(memory.recorded(), 0, "sem escrita de facto");
        match dispatch.outcome() {
            ToolOutcome::Denied { rule_id, .. } => {
                assert_eq!(rule_id.as_str(), "mem-no-duplicate");
            }
            other => return Err(format!("esperava Denied, veio {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn allowed_write_runs_the_executor_once() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::default();
        let probe = CommitProbe {
            calls: AtomicUsize::new(0),
        };
        let (req, rules, cwd) = (request(), rules()?, cwd()?);
        let dispatch = enforce_memory_write(
            &state_with_recall(),
            MemoryWriteRequest {
                cwd: &cwd,
                req: &req,
                memory: &memory,
                rules: &rules,
                now_millis: 0,
                tool: &probe,
            },
        )?;
        assert!(dispatch.ran());
        assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
        assert_eq!(dispatch.outcome(), ToolOutcome::Ok);
        Ok(())
    }

    #[test]
    fn write_without_recall_is_not_executed() -> Result<(), Box<dyn std::error::Error>> {
        let memory = FakeMemory::default();
        let probe = CommitProbe {
            calls: AtomicUsize::new(0),
        };
        let (req, rules, cwd) = (request(), rules()?, cwd()?);
        let dispatch = enforce_memory_write(
            &State::initial(),
            MemoryWriteRequest {
                cwd: &cwd,
                req: &req,
                memory: &memory,
                rules: &rules,
                now_millis: 0,
                tool: &probe,
            },
        )?;
        assert!(!dispatch.ran());
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert!(matches!(dispatch.outcome(), ToolOutcome::Denied { .. }));
        assert_eq!(
            dispatch.outcome().rule_id().map(RuleId::as_str),
            Some("mem-recall-before-write"),
            "a recusa tem de nomear a regra (DF10/DF11)"
        );
        Ok(())
    }

    #[test]
    fn tool_use_is_the_nominal_command() -> Result<(), Box<dyn std::error::Error>> {
        let use_ = memory_write_use(&cwd()?);
        assert_eq!(use_.name, ToolName::MemoryWrite);
        Ok(())
    }
}
