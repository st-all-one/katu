//! Cliente do kernel na TUI (`KERNEL_SURFACE` F1/F2): só envia comandos e consome eventos.
//!
//! O `KernelClient` **não** possui `Runtime`/`Provider`/`Session` (K2): traduz as ações da UI em
//! [`Command`]s e injeta os [`Event`]s de volta no `App`. É **não bloqueante** (F2): `send` só
//! enfileira e `poll` drena o que está pronto, pelo que o loop da UI fica livre para tratar input
//! (rato/resize/cópia) e redesenhar enquanto o turno corre.

use std::time::Duration;

use katu_core::api::{Command, Event, KernelHandle, RecvError, SendError};
use katu_core::ports::Flag;
use katu_tui::{Handler, Painter, Update};

/// Período de espera por um evento do kernel no arranque (ms): o `request` bloqueia até `Done`.
const EVENT_POLL_MS: u64 = 20;

/// Cliente do kernel: a superfície da TUI.
pub(super) struct KernelClient {
    handle: KernelHandle,
    flag: Flag,
    granted_by: String,
    /// Há um comando de trabalho em curso (o kernel ainda não emitiu `Done`).
    busy: bool,
}

impl KernelClient {
    /// Constrói o cliente sobre o extremo da superfície.
    pub(super) fn new(handle: KernelHandle, granted_by: String) -> Self {
        let _span = katu_core::trace_fn!("tui::handler::new");

        let flag = handle.cancel_flag();
        Self {
            handle,
            flag,
            granted_by,
            busy: false,
        }
    }

    /// Envia um comando e recolhe os eventos até `Done` (bloqueante; usado no arranque).
    pub(super) fn request(&self, command: Command) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::request");

        let mut collected = Vec::new();
        if let Err(error) = self.handle.send(command) {
            collected.push(send_error(error));
            return collected;
        }
        loop {
            match self.handle.recv(Duration::from_millis(EVENT_POLL_MS)) {
                Ok(Event::Done) => {
                    collected.push(Event::Done);
                    return collected;
                }
                Ok(event) => collected.push(event),
                Err(RecvError::Timeout) => {}
                Err(RecvError::Closed) => {
                    collected.push(Update::Error("kernel terminou inesperadamente".to_string()));
                    return collected;
                }
            }
        }
    }
}

impl Handler for KernelClient {
    fn cancel_flag(&self) -> Flag {
        let _span = katu_core::trace_fn!("tui::handler::cancel_flag");

        self.flag.clone()
    }

    fn send(&mut self, command: Command) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::send");

        let done = produces_done(&command);
        match self.handle.send(command) {
            Ok(()) => {
                if done {
                    self.busy = true;
                }
                Vec::new()
            }
            Err(error) => vec![send_error(error)],
        }
    }

    fn poll(&mut self, painter: &mut Painter<'_>) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::poll");

        let mut collected = Vec::new();
        // `recv(ZERO)` devolve já o que está pronto e distingue o fecho do canal (F2/K5).
        loop {
            match self.handle.recv(Duration::ZERO) {
                Ok(Event::Live(live)) => painter.live(live),
                Ok(Event::ApprovalRequest(request)) => {
                    let grant = painter.challenge(request, &self.granted_by);
                    let _sent = self.handle.send(Command::Approval(grant));
                }
                Ok(Event::Done) => {
                    self.busy = false;
                    collected.push(Event::Done);
                }
                Ok(event) => collected.push(event),
                Err(RecvError::Timeout) => break,
                Err(RecvError::Closed) => {
                    if self.busy {
                        self.busy = false;
                        collected
                            .push(Update::Error("kernel terminou inesperadamente".to_string()));
                    }
                    break;
                }
            }
        }
        collected
    }

    fn busy(&self) -> bool {
        let _span = katu_core::trace_fn!("tui::handler::busy");

        self.busy
    }
}

/// `true` se o comando produz um evento terminal `Done` (os de controlo não produzem).
fn produces_done(command: &Command) -> bool {
    let _span = katu_core::trace_fn!("tui::handler::produces_done");

    !matches!(
        command,
        Command::Cancel | Command::Steer(_) | Command::Approval(_)
    )
}

/// Traduz um erro de envio do protocolo num evento visível (K4/K5).
fn send_error(error: SendError) -> Update {
    let _span = katu_core::trace_fn!("tui::handler::send_error");

    match error {
        SendError::Full => Event::Busy("kernel ocupado; tente de novo".to_string()),
        SendError::Closed => Update::Error("kernel terminou inesperadamente".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use katu_core::api::{COMMAND_QUEUE, Command, Event, channel};
    use katu_core::provider::Thinking;
    use katu_tui::Handler as _;

    use super::KernelClient;

    #[test]
    fn request_collects_events_until_done() {
        let (bus, handle) = channel();
        let responder = std::thread::spawn(move || {
            if let Ok(command) = bus.recv() {
                assert!(matches!(command, Command::SetThinking(Thinking::Off)));
                bus.publish(Event::Info("ok".to_string()));
                bus.publish(Event::Done);
            }
        });
        let client = KernelClient::new(handle, "me".to_string());
        let updates = client.request(Command::SetThinking(Thinking::Off));
        assert!(matches!(updates.first(), Some(Event::Info(_))));
        assert!(matches!(updates.last(), Some(Event::Done)));
        responder.join().ok();
    }

    #[test]
    fn send_marks_busy_and_control_commands_do_not() {
        let (_bus, handle) = channel();
        let mut client = KernelClient::new(handle, "me".to_string());
        assert!(!client.busy());
        assert!(client.send(Command::Submit("olá".to_string())).is_empty());
        assert!(client.busy(), "um comando de trabalho fica pendente");
        let _ignored = client.send(Command::Steer("vai".to_string()));
        assert!(client.busy(), "o steering não altera a pendência");
    }

    #[test]
    fn a_full_queue_reports_busy_instead_of_blocking() {
        let (_bus, handle) = channel();
        let mut client = KernelClient::new(handle, "me".to_string());
        for _ in 0..COMMAND_QUEUE {
            assert!(client.send(Command::Compact).is_empty());
        }
        let updates = client.send(Command::Compact);
        assert!(matches!(updates.first(), Some(Event::Busy(_))));
    }
}
