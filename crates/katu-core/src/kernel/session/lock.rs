//! Lock de turno por sessão (L-Q6): só um processo escreve no mesmo log de cada vez.
//!
//! O estado `turn_open` do log já recusa **dois** turnos no mesmo processo; o que faltava era o
//! caso de dois processos (`katu run --resume` sobre a mesma sessão). O lock reutiliza o log sem
//! o duplicar: um ficheiro `turn.lock` no diretório da sessão com `pid` + instante de aquisição.
//!
//! Um lock de **outro** processo ainda fresco recusa com [`SessionError::TurnLocked`] (categoria
//! `conflict`, com o `pid` e a idade). Um lock obsoleto (processo morto ou idade acima do limiar)
//! é adotado; um lock do próprio processo também, o que mantém os testes e o replay dentro do
//! mesmo processo transparentes.

use std::path::PathBuf;

use super::{Session, SessionError};
use crate::diag::{Level, events};
use crate::ports::FsError;

/// Nome do ficheiro de lock dentro do diretório da sessão.
const TURN_LOCK_FILE: &str = "turn.lock";

/// Idade (ms) a partir da qual um lock de outro processo é considerado obsoleto.
///
/// Conservador: um turno real pode demorar, mas um `SIGKILL` deixa o lock para trás. Passado este
/// limiar a retomada adota-o (o `pid` já não fala) e reconcilia o turno aberto (L-Q1).
const TURN_LOCK_STALE_MS: u64 = 300_000;

/// Conteúdo do lock: `pid` + instante de aquisição, cada um numa linha.
struct LockOwner {
    pid: u32,
    acquired_ms: u64,
}

impl Session<'_> {
    /// Caminho do ficheiro de lock do turno.
    fn turn_lock_path(&self) -> PathBuf {
        let _span = crate::trace_fn!("kernel::session::lock::turn_lock_path");

        self.dir.join(TURN_LOCK_FILE)
    }

    /// Adquire o lock do turno (L-Q6). Recusa se outro processo o detiver e o lock for fresco.
    ///
    /// # Errors
    /// [`SessionError::TurnLocked`] se outro `pid` detiver o turno; [`SessionError`] se a escrita
    /// falhar.
    pub fn acquire_turn_lock(&self, now_millis: u64) -> Result<(), SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::AGENT_TURN_LOCK,
            "kernel::session::acquire_turn_lock"
        );
        let pid = std::process::id();
        let path = self.turn_lock_path();
        if let Ok(bytes) = self.fs.read(&path)
            && let Some(owner) = parse_lock(&String::from_utf8_lossy(&bytes))
        {
            let age_ms = now_millis.saturating_sub(owner.acquired_ms);
            if owner.pid != pid && age_ms < TURN_LOCK_STALE_MS {
                crate::event!(
                    Level::Warn,
                    events::AGENT_TURN_LOCK,
                    "pid" => owner.pid,
                    "age_ms" => age_ms,
                    "acquired" => false
                );
                return Err(SessionError::TurnLocked {
                    pid: owner.pid,
                    age_ms,
                });
            }
        }
        let body = format!("{pid}\n{now_millis}\n");
        self.fs.write_atomic(&path, body.as_bytes())?;
        Ok(())
    }

    /// Liberta o lock do turno. Um lock inexistente é um no-op (o fecho tem de ser idempotente).
    ///
    /// # Errors
    /// [`SessionError`] se a remoção falhar por outro motivo que não "não encontrado".
    pub fn release_turn_lock(&self) -> Result<(), SessionError> {
        let _span = crate::trace_fn!("kernel::session::lock::release_turn_lock");

        match self.fs.remove(&self.turn_lock_path()) {
            Ok(()) | Err(FsError::NotFound) => Ok(()),
            Err(error) => Err(SessionError::Fs(error)),
        }
    }
}

/// Interpreta o conteúdo do lock (`pid\nacquired_ms\n`); `None` se estiver malformado.
fn parse_lock(text: &str) -> Option<LockOwner> {
    let _span = crate::trace_fn!("kernel::session::lock::parse_lock");

    let mut lines = text.lines();
    let pid = lines.next()?.trim().parse().ok()?;
    let acquired_ms = lines.next()?.trim().parse().ok()?;
    Some(LockOwner { pid, acquired_ms })
}
