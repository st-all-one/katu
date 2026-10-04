//! Canais do protocolo (`KERNEL_SURFACE` §2.3): a superfície envia comandos, o kernel publica eventos.
//!
//! O par `(KernelBus, KernelHandle)` é o transporte **em memória** entre a thread do kernel e a
//! superfície. A fronteira é desenhada para nunca bloquear a superfície (K5): os comandos usam uma
//! fila limitada com `try_send` (fila cheia = kernel ocupado, não *deadlock*) e os eventos são
//! drenados sem bloquear no loop da superfície. Em F4 o mesmo protocolo viaja num socket.

use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::Duration;

use crate::ports::Flag;

use super::{Command, Event};

/// Capacidade da fila de comandos (backpressure explícita; K5).
pub const COMMAND_QUEUE: usize = 32;

/// Erro ao enviar um comando ao kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// A fila está cheia: o kernel ainda não consumiu o comando anterior.
    Full,
    /// O kernel fechou (thread terminou).
    Closed,
}

/// Erro ao receber um comando/evento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError {
    /// Não há nada dentro do prazo (a superfície continua a viver).
    Timeout,
    /// O outro extremo fechou.
    Closed,
}

/// Extremo da **superfície**: envia comandos e consome eventos.
pub struct KernelHandle {
    commands: SyncSender<Command>,
    events: Receiver<Event>,
    cancel: Flag,
    continue_once: Flag,
}

impl KernelHandle {
    /// Envia um comando **sem bloquear** (K5): fila cheia devolve [`SendError::Full`].
    ///
    /// # Errors
    /// [`SendError::Full`] com o kernel ocupado; [`SendError::Closed`] se a thread terminou.
    pub fn send(&self, command: Command) -> Result<(), SendError> {
        let _span = crate::trace_fn!("api::handle::send");

        match self.commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SendError::Full),
            Err(TrySendError::Disconnected(_)) => Err(SendError::Closed),
        }
    }

    /// Recebe o próximo evento, esperando até `timeout` (o CLI bloqueia aqui; a TUI usa [`drain`]).
    ///
    /// [`drain`]: KernelHandle::drain
    ///
    /// # Errors
    /// [`RecvError::Timeout`] se nada chegar no prazo; [`RecvError::Closed`] se o kernel fechou.
    pub fn recv(&self, timeout: Duration) -> Result<Event, RecvError> {
        let _span = crate::trace_fn!("api::handle::recv");

        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(event),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(RecvError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RecvError::Closed),
        }
    }

    /// Drena até `max` eventos **sem** bloquear (o loop da superfície chama-o a cada quadro).
    pub fn drain(&self, max: usize) -> Vec<Event> {
        let _span = crate::trace_fn!("api::handle::drain");

        let mut events = Vec::new();
        while events.len() < max {
            match self.events.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    /// Pede o cancelamento: escreve a flag partilhada (vista pelo transporte/tools) **e** envia o
    /// comando cooperativo (K8).
    ///
    /// # Errors
    /// Como [`send`](KernelHandle::send); a flag é escrita mesmo que o comando não caiba.
    pub fn request_cancel(&self) -> Result<(), SendError> {
        let _span = crate::trace_fn!("api::handle::request_cancel");

        self.cancel.request();
        self.send(Command::Cancel)
    }

    /// Cópia da flag de cancelamento (partilhada com o transporte e as tools).
    #[must_use]
    pub fn cancel_flag(&self) -> Flag {
        let _span = crate::trace_fn!("api::handle::cancel_flag");

        self.cancel.clone()
    }

    /// Pede **um** passo extra antes do fim natural (`S1/PI_GAINS`): arma a flag one-shot **e** envia
    /// o comando (no-op fora de um turno).
    ///
    /// # Errors
    /// Como [`send`](KernelHandle::send); a flag é armada mesmo que o comando não caiba.
    pub fn request_continue(&self) -> Result<(), SendError> {
        let _span = crate::trace_fn!("api::handle::request_continue");

        self.continue_once.request();
        self.send(Command::Continue)
    }
}

/// Extremo do **kernel**: lê comandos e publica eventos (usado pela thread do kernel).
pub struct KernelBus {
    commands: Receiver<Command>,
    events: mpsc::Sender<Event>,
    cancel: Flag,
    continue_once: Flag,
}

/// Handle de **publicação** clonável e `Send + Sync` (`P1/PI_GAINS`).
///
/// O [`KernelBus`] não é `Sync` (segura o `Receiver` dos comandos), mas o progresso das tools
/// corre em threads paralelas e só precisa de **publicar** eventos: este handle leve isola o
/// `Sender` (que é `Send + Sync`).
#[derive(Clone)]
pub struct Publisher {
    events: mpsc::Sender<Event>,
}

impl Publisher {
    /// Publica um evento; `false` se a superfície já fechou.
    pub fn publish(&self, event: Event) -> bool {
        let _span = crate::trace_fn!("api::handle::publisher_publish");

        self.events.send(event).is_ok()
    }
}

impl KernelBus {
    /// Recebe o próximo comando, bloqueando até chegar um (K4: o kernel é dirigido por comandos).
    ///
    /// # Errors
    /// [`RecvError::Closed`] se a superfície fechou (o kernel deve terminar).
    pub fn recv(&self) -> Result<Command, RecvError> {
        let _span = crate::trace_fn!("api::handle::bus_recv");

        self.commands.recv().map_err(|_| RecvError::Closed)
    }

    /// Recebe um comando sem bloquear (o kernel sonda durante um turno, ex.: `Cancel`).
    #[must_use]
    pub fn try_recv(&self) -> Option<Command> {
        let _span = crate::trace_fn!("api::handle::bus_try_recv");

        self.commands.try_recv().ok()
    }

    /// Publica um evento; `false` se a superfície já fechou (o kernel decide continuar ou parar).
    pub fn publish(&self, event: Event) -> bool {
        let _span = crate::trace_fn!("api::handle::publish");

        self.events.send(event).is_ok()
    }

    /// Cópia da flag de cancelamento (o loop do kernel lê-a nas fronteiras; K8).
    #[must_use]
    pub fn cancel_flag(&self) -> Flag {
        let _span = crate::trace_fn!("api::handle::cancel_flag");

        self.cancel.clone()
    }

    /// Cópia da flag de `Command::Continue` (one-shot; `S1/PI_GAINS`).
    #[must_use]
    pub fn continue_flag(&self) -> Flag {
        let _span = crate::trace_fn!("api::handle::continue_flag");

        self.continue_once.clone()
    }

    /// Handle `Send + Sync` para publicar eventos de threads paralelas (`P1/PI_GAINS`).
    #[must_use]
    pub fn publisher(&self) -> Publisher {
        let _span = crate::trace_fn!("api::handle::publisher");

        Publisher {
            events: self.events.clone(),
        }
    }
}

/// Cria o par (kernel, superfície) sobre canais em memória.
#[must_use]
pub fn channel() -> (KernelBus, KernelHandle) {
    let _span = crate::trace_fn!("api::handle::channel");

    let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE);
    let (event_tx, event_rx) = mpsc::channel();
    let cancel = Flag::new();
    let continue_once = Flag::new();
    let bus = KernelBus {
        commands: command_rx,
        events: event_tx,
        cancel: cancel.clone(),
        continue_once: continue_once.clone(),
    };
    let handle = KernelHandle {
        commands: command_tx,
        events: event_rx,
        cancel,
        continue_once,
    };
    (bus, handle)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{COMMAND_QUEUE, RecvError, SendError, channel};
    use crate::api::{Command, Event};
    use crate::ports::Cancel as _;

    #[test]
    fn a_command_round_trips_to_the_kernel() {
        let (bus, handle) = channel();
        assert!(handle.send(Command::Submit("olá".to_string())).is_ok());
        assert!(matches!(
            bus.recv(),
            Ok(Command::Submit(text)) if text == "olá"
        ));
    }

    #[test]
    fn a_full_queue_is_reported_instead_of_blocking() {
        let (bus, handle) = channel();
        for _ in 0..COMMAND_QUEUE {
            assert!(handle.send(Command::Cancel).is_ok());
        }
        assert_eq!(handle.send(Command::Cancel), Err(SendError::Full));
        for _ in 0..COMMAND_QUEUE {
            assert!(bus.try_recv().is_some());
        }
    }

    #[test]
    fn an_event_round_trips_to_the_surface() {
        let (bus, handle) = channel();
        assert!(bus.publish(Event::Done));
        assert_eq!(handle.drain(8), vec![Event::Done]);
    }

    #[test]
    fn cancel_writes_the_shared_flag_and_sends_the_command() {
        let (bus, handle) = channel();
        let flag = handle.cancel_flag();
        assert!(!flag.cancelled());
        assert!(handle.request_cancel().is_ok());
        assert!(flag.cancelled(), "a flag é escrita mesmo antes do comando");
        assert!(matches!(bus.try_recv(), Some(Command::Cancel)));
    }

    #[test]
    fn continue_arms_the_flag_and_sends_the_command() {
        let (bus, handle) = channel();
        let flag = bus.continue_flag();
        assert!(!flag.take());
        assert!(handle.request_continue().is_ok());
        assert!(flag.take(), "a flag é armada mesmo antes do comando");
        assert!(matches!(bus.try_recv(), Some(Command::Continue)));
    }

    #[test]
    fn a_closed_kernel_is_detected_on_both_ends() {
        let (bus, handle) = channel();
        drop(handle);
        assert!(matches!(bus.recv(), Err(RecvError::Closed)));
        assert!(
            !bus.publish(Event::Done),
            "sem superfície, o evento não é entregue"
        );
    }

    #[test]
    fn a_timeout_is_not_a_close() {
        let (_bus, handle) = channel();
        assert!(matches!(
            handle.recv(Duration::from_millis(1)),
            Err(RecvError::Timeout)
        ));
    }

    #[test]
    fn a_zero_timeout_drains_ready_events_without_blocking() {
        let (bus, handle) = channel();
        assert!(bus.publish(Event::Done));
        assert_eq!(handle.recv(Duration::ZERO), Ok(Event::Done));
        assert!(
            matches!(handle.recv(Duration::ZERO), Err(RecvError::Timeout)),
            "sem eventos, um prazo zero devolve imediatamente"
        );
    }
}
