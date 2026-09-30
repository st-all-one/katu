//! Event bus mínimo (E04-T06): observadores (`emit`) e around-middleware (`waterfall`).
//!
//! Regras explícitas (§43.5, §45):
//!
//! - **Observadores** só observam: recebem o evento e não afetam o fluxo. Um erro num observador
//!   não existe por construção (a assinatura é infalível); é essa a forma de "conter exceções".
//! - **Middleware** envolve o processamento e **tem de chamar `next`**. Um middleware que devolve
//!   `Ok(())` sem chamar `next` é um bug (um observador a fingir-se de middleware) e falha o
//!   `dispatch` com [`HandlerError::SkippedNext`].
//! - Um middleware que quer **curto-circuitar** devolve `Err(..)` (sem chamar `next`): é uma
//!   decisão, não um esquecimento.
//!
//! Não há contentor de DI geral: o bus é uma lista de referências com um terminal.

use crate::diag::{Level, events};

/// Resultado de um handler do bus.
pub type HandlerResult = Result<(), HandlerError>;

/// Erro de um handler do bus.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HandlerError {
    /// O middleware devolveu `Ok(())` sem chamar `next()`.
    #[error("middleware `{name}` devolveu Ok sem chamar next()")]
    SkippedNext {
        /// Nome do middleware.
        name: &'static str,
    },
    /// O middleware chamou `next()` mais de uma vez.
    #[error("next() chamado mais de uma vez")]
    CalledTwice,
    /// O middleware falhou (curto-circuito explícito).
    #[error("handler `{name}`: {message}")]
    Failed {
        /// Nome do middleware.
        name: &'static str,
        /// Mensagem estável.
        message: String,
    },
}

/// Observador: recebe eventos, sem afetar o fluxo.
pub trait Observer<E>: Send + Sync {
    /// Observa um evento.
    fn on_event(&self, event: &E);
}

/// Middleware de *waterfall*: envolve o processamento a jusante.
pub trait Middleware<E>: Send + Sync {
    /// Nome estável (para diagnósticos e erros).
    fn name(&self) -> &'static str;

    /// Envolve o evento. Deve chamar `next` para continuar, ou devolver `Err` para cortar.
    fn around(&self, event: &E, next: &mut Next<'_, E>) -> HandlerResult;
}

/// Continuação do *waterfall*, entregue a cada [`Middleware`].
pub struct Next<'a, E> {
    downstream: &'a mut dyn FnMut(&E) -> HandlerResult,
    calls: u32,
}

impl<'a, E> Next<'a, E> {
    /// Cria a continuação (uso interno do [`EventBus`]).
    fn new(downstream: &'a mut dyn FnMut(&E) -> HandlerResult) -> Self {
        let _span = crate::trace_fn!("kernel::bus::new");

        Self {
            downstream,
            calls: 0,
        }
    }

    /// Executa o resto da cadeia. Só pode ser chamado uma vez.
    ///
    /// # Errors
    /// [`HandlerError::CalledTwice`] numa segunda chamada; caso contrário propaga o erro a jusante.
    pub fn run(&mut self, event: &E) -> HandlerResult {
        let _span = crate::trace_fn!("kernel::bus::run");

        self.calls = self.calls.saturating_add(1);
        if self.calls > 1 {
            return Err(HandlerError::CalledTwice);
        }
        (self.downstream)(event)
    }

    /// Número de vezes que [`Next::run`] foi chamado.
    #[must_use]
    pub fn calls(&self) -> u32 {
        let _span = crate::trace_fn!("kernel::bus::calls");

        self.calls
    }
}

/// Bus de eventos: observadores + cadeia de middleware + terminal.
pub struct EventBus<'a, E> {
    observers: Vec<&'a dyn Observer<E>>,
    middleware: Vec<&'a dyn Middleware<E>>,
    terminal: &'a dyn Fn(&E) -> HandlerResult,
}

impl<'a, E> EventBus<'a, E> {
    /// Cria um bus com o terminal dado (o fim da cadeia).
    pub fn new(terminal: &'a dyn Fn(&E) -> HandlerResult) -> Self {
        let _span = crate::trace_fn!("kernel::bus::new");

        Self {
            observers: Vec::new(),
            middleware: Vec::new(),
            terminal,
        }
    }

    /// Acrescenta um observador.
    #[must_use]
    pub fn with_observer(mut self, observer: &'a dyn Observer<E>) -> Self {
        let _span = crate::trace_fn!("kernel::bus::with_observer");

        self.observers.push(observer);
        self
    }

    /// Acrescenta um middleware ao fim da cadeia.
    #[must_use]
    pub fn with_middleware(mut self, middleware: &'a dyn Middleware<E>) -> Self {
        let _span = crate::trace_fn!("kernel::bus::with_middleware");

        self.middleware.push(middleware);
        self
    }

    /// Notifica todos os observadores (infalível).
    pub fn emit(&self, event: &E) {
        let _span = crate::fn_span!(Level::Trace, events::BUS_DELIVER, "kernel::bus::emit");
        for observer in &self.observers {
            observer.on_event(event);
        }
    }

    /// Notifica os observadores e corre a cadeia de middleware até ao terminal.
    ///
    /// # Errors
    /// [`HandlerError`] se um middleware violar a regra de `next` ou devolver erro.
    pub fn dispatch(&self, event: &E) -> HandlerResult {
        let _span = crate::fn_span!(Level::Debug, events::BUS_PUBLISH, "kernel::bus::dispatch");
        self.emit(event);
        self.invoke(0, event)
    }

    /// Invoca o middleware em `index` (ou o terminal, no fim da cadeia).
    fn invoke(&self, index: usize, event: &E) -> HandlerResult {
        let _span = crate::trace_fn!("kernel::bus::invoke");

        let Some(middleware) = self.middleware.get(index) else {
            return (self.terminal)(event);
        };
        let mut downstream = |event: &E| self.invoke(index.saturating_add(1), event);
        let mut next = Next::new(&mut downstream);
        let result = middleware.around(event, &mut next);
        match result {
            Ok(()) if next.calls() == 0 => Err(HandlerError::SkippedNext {
                name: middleware.name(),
            }),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;
