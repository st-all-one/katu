//! Progresso **efémero** de uma tool longa (`P1/PI_GAINS`).
//!
//! O output incremental de um `bash` é mostrado ao utilizador enquanto corre, mas **nunca** entra
//! no log nem no contexto do modelo (como o [`Live`](crate::api::Live) do protocolo): é só sinal
//! de vida. A porta é pura — o núcleo não conhece o terminal.

/// Consumidor de fragmentos de output de uma tool (implementado pela borda).
pub trait Progress: Send + Sync {
    /// Entrega um fragmento de output da tool `name` (texto *lossy*).
    fn chunk(&self, name: &str, text: &str);
}

/// Não emite progresso: a borda não interativa (`katu run`) e os testes que o não exercitam.
#[derive(Debug, Default)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn chunk(&self, _name: &str, _text: &str) {
        let _span = crate::trace_fn!("ports::progress::chunk");
    }
}

/// Instância partilhada do no-op (para construir `&dyn Progress` sem progresso).
pub static NO_PROGRESS: NoProgress = NoProgress;
