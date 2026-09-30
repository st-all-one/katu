//! Caminhos de memória do runtime (E03-T03/T07): recall e escrita pelo gate §42.
//!
//! Vive num módulo filho para manter `runtime.rs` sob o teto de linhas. Tudo passa pelo caminho
//! único `logar → política → efeito` da `Session`; a memória é a de primeira classe (in-process).

use katu_core::kernel::{CallContext, Dispatch, MemoryWriteRequest, memory_recall_use};
use katu_core::memory::{Anchor, NoteType, PreWriteReq, RecallReq};
use katu_tools::recall::RecallTool;
use katu_tools::write::WriteNoteTool;

use super::{Runtime, RuntimeError};

impl Runtime<'_> {
    /// Consulta a memória pelo caminho §42 (logado antes de executar; policy-gated).
    ///
    /// # Errors
    /// [`RuntimeError`] se a transição, o custo ou a política falharem.
    pub(crate) fn recall(&mut self, query: &str, limit: usize) -> Result<Dispatch, RuntimeError> {
        let call = self.call("recall");
        let tool = RecallTool {
            memory: &self.memory,
            req: RecallReq {
                query: query.to_string(),
                limit,
            },
        };
        let use_ = memory_recall_use(&self.cwd);
        let now = self.clock.now().as_millis();
        self.session
            .tool_call(
                call,
                &use_,
                CallContext {
                    rules: &self.rules,
                    now_millis: now,
                    tool: &tool,
                },
            )
            .map_err(RuntimeError::from)
    }

    /// Regista uma nota: recall prévio (protocolo) + escrita pelo gate de E05 (ordem §42).
    ///
    /// Reservado ao caminho de escrita (agente/`kd`); hoje exercido pelos testes do runtime — a
    /// superfície `memo` só consulta (E20-T06).
    ///
    /// # Errors
    /// [`RuntimeError`] se o recall, a transição, o custo ou a política falharem.
    #[allow(
        dead_code,
        reason = "caminho de escrita reservado (agente/kd); exercido pelos testes do runtime"
    )]
    pub(crate) fn remember(&mut self, req: &PreWriteReq) -> Result<Dispatch, RuntimeError> {
        self.recall(&req.statement, 5)?;
        let call = self.call("write");
        let tool = WriteNoteTool {
            memory: &self.memory,
            req: req.clone(),
        };
        let now = self.clock.now().as_millis();
        let request = MemoryWriteRequest {
            cwd: &self.cwd,
            req,
            memory: &self.memory,
            rules: &self.rules,
            now_millis: now,
            tool: &tool,
        };
        Ok(self.session.memory_write(call, request)?)
    }

    /// Constrói o `PreWriteReq` de uma nota simples (afirmação; tipo e âncora opcionais).
    #[allow(
        dead_code,
        reason = "construtor do caminho de escrita reservado; exercido pelos testes do runtime"
    )]
    pub(crate) fn note(statement: &str, note_type: NoteType, anchor: Option<&str>) -> PreWriteReq {
        PreWriteReq {
            statement: statement.to_string(),
            note_type,
            anchor: anchor.map(Anchor::new),
            body: String::new(),
        }
    }
}
