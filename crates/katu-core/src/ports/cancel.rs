//! Cancelamento cooperativo (L-P1/L-P3).
//!
//! O turno é **interrompível em qualquer instante**: a flag é consultada nas fronteiras (passo,
//! tool, leitura do transporte) e dentro das tools longas (o `bash` mata o grupo de processos ao
//! cancelar). O tipo é uma porta pura — o núcleo não conhece o terminal nem a UI.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Fonte de cancelamento cooperativo (a UI escreve, o loop/tools leem).
pub trait Cancel: Send + Sync {
    /// `true` se o utilizador pediu para cancelar o turno.
    fn cancelled(&self) -> bool;
}

/// Nunca cancela: a borda não interativa (`katu run`) e os testes que não exercitam a flag.
#[derive(Debug, Default)]
pub struct Never;

impl Cancel for Never {
    fn cancelled(&self) -> bool {
        let _span = crate::trace_fn!("ports::cancel::cancelled");

        false
    }
}

/// Flag de cancelamento partilhada entre a UI, o loop e as tools (clone barato).
#[derive(Debug, Clone, Default)]
pub struct Flag {
    inner: Arc<AtomicBool>,
}

impl Flag {
    /// Cria uma flag por cancelar.
    #[must_use]
    pub fn new() -> Self {
        let _span = crate::trace_fn!("ports::cancel::new");

        Self::default()
    }

    /// Pede o cancelamento (idempotente). A UI chama-o no `Esc`/`Ctrl-C`.
    pub fn request(&self) {
        let _span = crate::trace_fn!("ports::cancel::request");

        self.inner.store(true, Ordering::SeqCst);
    }

    /// Consome um pedido **one-shot**: devolve `true` e limpa a flag (`S1/PI_GAINS`).
    ///
    /// Usado pela flag de `Command::Continue`: cada pedido vale **um** passo extra. Para o
    /// cancelamento, usa-se [`Cancel::cancelled`] (nunca se consome).
    pub fn take(&self) -> bool {
        let _span = crate::trace_fn!("ports::cancel::take");

        self.inner.swap(false, Ordering::SeqCst)
    }
}

impl Cancel for Flag {
    fn cancelled(&self) -> bool {
        let _span = crate::trace_fn!("ports::cancel::cancelled");

        self.inner.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cancel, Flag, Never};

    #[test]
    fn flag_starts_clear_and_requests_once() {
        let flag = Flag::new();
        assert!(!flag.cancelled());
        flag.request();
        assert!(flag.cancelled());
        flag.request();
        assert!(flag.cancelled(), "pedir duas vezes é idempotente");
    }

    #[test]
    fn never_never_cancels() {
        assert!(!Never.cancelled());
    }

    #[test]
    fn take_consumes_a_one_shot_request() {
        let flag = Flag::new();
        assert!(!flag.take());
        flag.request();
        assert!(flag.take());
        assert!(!flag.take(), "one-shot: o segundo take é vazio");
    }
}
