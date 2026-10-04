//! Adaptador do kernel ao protocolo (`KERNEL_SURFACE` F1): atividade efémera e aprovação.
//!
//! O `BusSink` é o `ActivitySink` do turno quando o kernel corre na sua thread: em vez de desenhar
//! no terminal, **publica** eventos no canal e espera a **resposta** de aprovação pelo mesmo canal
//! (request/response, fail-closed). O cancelamento lê a flag partilhada (K8).

use std::collections::VecDeque;
use std::time::Duration;

use katu_core::api::{ApprovalGrant, ApprovalRequest, Command, Event, KernelBus, Live, Publisher};
use katu_core::diag::{Level, events};
use katu_core::ports::{Cancel as _, Flag, Progress};

use crate::agent::{Activity, ActivitySink, Approval, ApprovalPrompt};

/// Período de sondagem do canal enquanto se espera por uma aprovação (ms).
const APPROVAL_POLL_MS: u64 = 10;

/// `ActivitySink` que publica no canal do kernel e pede aprovação por request/response.
pub(crate) struct BusSink<'a> {
    bus: &'a KernelBus,
    flag: Flag,
    continue_once: Flag,
    steer: VecDeque<String>,
}

impl<'a> BusSink<'a> {
    /// Constrói o sink sobre o extremo do kernel.
    pub(crate) fn new(bus: &'a KernelBus) -> Self {
        let _span = katu_core::trace_fn!("tui::kernel::sink::new");

        Self {
            bus,
            flag: bus.cancel_flag(),
            continue_once: bus.continue_flag(),
            steer: VecDeque::new(),
        }
    }

    /// Cópia da flag de cancelamento (partilhada com o transporte e as tools; K8).
    pub(crate) fn cancel_flag(&self) -> Flag {
        let _span = katu_core::trace_fn!("tui::kernel::sink::cancel_flag");

        self.flag.clone()
    }

    /// Handle de publicação `Send + Sync` (para o progresso das tools; `P1/PI_GAINS`).
    pub(crate) fn publisher(&self) -> Publisher {
        let _span = katu_core::trace_fn!("tui::kernel::sink::publisher");

        self.bus.publisher()
    }

    /// Publica um evento (ignora se a superfície já fechou).
    pub(crate) fn publish(&self, event: Event) {
        let _span = katu_core::trace_fn!("tui::kernel::sink::publish");

        self.bus.publish(event);
    }

    /// Pede aprovação humana pelo protocolo e espera a resposta (fail-closed).
    ///
    /// Publica `ApprovalRequest` e bloqueia até chegar `Command::Approval`; `Cancel`/`Steer` são
    /// processados durante a espera. Sem resposta (canal fechado ou cancelamento) devolve `None`.
    pub(crate) fn ask(&mut self, tool: &str, rule: &str, scope: &str) -> Option<ApprovalGrant> {
        let _span = katu_core::trace_fn!("tui::kernel::sink::ask");

        self.publish(Event::ApprovalRequest(ApprovalRequest {
            tool: tool.to_string(),
            rule: rule.to_string(),
            scope: scope.to_string(),
        }));
        loop {
            if self.flag.cancelled() {
                return None;
            }
            match self.bus.try_recv() {
                Some(Command::Approval(grant)) => return grant,
                Some(Command::Cancel) => {
                    self.flag.request();
                    return None;
                }
                Some(Command::Steer(text)) => self.steer.push_back(text),
                Some(_) => {}
                None => std::thread::sleep(Duration::from_millis(APPROVAL_POLL_MS)),
            }
        }
    }

    /// Drena os comandos de controlo (`Cancel`/`Steer`) sem bloquear.
    fn drain_control(&mut self) {
        while let Some(command) = self.bus.try_recv() {
            match command {
                Command::Cancel => self.flag.request(),
                Command::Steer(text) => self.steer.push_back(text),
                _ => {}
            }
        }
    }
}

impl ActivitySink for BusSink<'_> {
    fn activity(&mut self, activity: Activity<'_>) {
        let _span = katu_core::trace_fn!("tui::kernel::sink::activity");

        let live = match activity {
            Activity::Text(delta) => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "text");
                Live::Text(delta.to_string())
            }
            Activity::Thinking(delta) => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "thinking");
                Live::Thinking(delta.to_string())
            }
            Activity::Tool { name, args } => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool");
                Live::Tool {
                    name: name.to_string(),
                    args: args.to_string(),
                }
            }
            Activity::ToolDone { name, summary } => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool_done");
                Live::ToolDone {
                    name: name.to_string(),
                    summary: summary.to_string(),
                }
            }
            Activity::Refused { rule, evidence, .. } => {
                katu_core::event!(Level::Warn, events::TUI_LIVE, "kind" => "refused", "rule" => rule);
                Live::Refused {
                    rule: rule.to_string(),
                    evidence: evidence.to_string(),
                }
            }
            Activity::Unavailable { control, .. } => {
                katu_core::event!(Level::Warn, events::TUI_LIVE, "kind" => "unavailable");
                Live::Unavailable {
                    control: control.to_string(),
                }
            }
        };
        self.publish(Event::Live(live));
    }

    fn tick(&mut self) {
        let _span = katu_core::trace_fn!("tui::kernel::sink::tick");

        // O kernel não sonda o terminal: a superfície é que trata do input e escreve a flag.
    }

    fn approve(&mut self, prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        let _span = katu_core::trace_fn!("tui::kernel::sink::approve");

        let grant = self.ask(
            prompt.tool,
            prompt.request.rule_id.as_str(),
            &prompt.request.scope,
        )?;
        katu_core::event!(
            Level::Warn,
            events::TUI_APPROVAL,
            "tool" => prompt.tool,
            "rule" => prompt.request.rule_id.as_str()
        );
        Some(Approval {
            reason: grant.reason,
            granted_by: grant.granted_by,
        })
    }

    fn cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("tui::kernel::sink::cancelled");

        self.flag.cancelled()
    }

    fn steer(&mut self) -> Option<String> {
        let _span = katu_core::trace_fn!("tui::kernel::sink::steer");

        self.drain_control();
        self.steer.pop_front()
    }

    fn continue_once(&mut self) -> bool {
        let _span = katu_core::trace_fn!("tui::kernel::sink::continue_once");

        self.continue_once.take()
    }
}

/// `Progress` que publica o output incremental como [`Live::ToolOutput`] no canal (`P1/PI_GAINS`).
///
/// É `Send + Sync`: as tools correm em threads paralelas e publicam **diretamente** (o
/// `mpsc::Sender` é seguro), sem tocar no `ActivitySink` do turno. Nada disto entra no log.
pub(crate) struct BusProgress {
    publisher: Publisher,
}

impl BusProgress {
    /// Constrói o progresso sobre um publicador do canal.
    pub(crate) fn new(publisher: Publisher) -> Self {
        let _span = katu_core::trace_fn!("tui::kernel::sink::progress_new");

        Self { publisher }
    }
}

impl Progress for BusProgress {
    fn chunk(&self, name: &str, text: &str) {
        let _span = katu_core::trace_fn!("tui::kernel::sink::progress");

        katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool_output");
        self.publisher.publish(Event::Live(Live::ToolOutput {
            name: name.to_string(),
            chunk: text.to_string(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use katu_core::api::{ApprovalGrant, Command, Event, Live, channel};
    use katu_core::ports::Progress;

    use super::{BusProgress, BusSink};
    use crate::agent::{Activity, ActivitySink};

    #[test]
    fn ask_round_trips_through_the_protocol() {
        let (bus, handle) = channel();
        let responder = std::thread::spawn(move || {
            while let Ok(event) = handle.recv(Duration::from_millis(50)) {
                if let Event::ApprovalRequest(request) = event {
                    assert_eq!(request.tool, "bash");
                    let _sent = handle.send(Command::Approval(Some(ApprovalGrant {
                        reason: "ok".to_string(),
                        granted_by: "me".to_string(),
                    })));
                    return;
                }
            }
        });
        let mut sink = BusSink::new(&bus);
        let grant = sink.ask("bash", "rule", "scope");
        assert_eq!(grant.map(|grant| grant.granted_by), Some("me".to_string()));
        responder.join().ok();
    }

    #[test]
    fn a_cancelled_approval_fails_closed() {
        let (bus, handle) = channel();
        handle.cancel_flag().request();
        let mut sink = BusSink::new(&bus);
        assert!(sink.ask("bash", "rule", "scope").is_none());
    }

    #[test]
    fn activity_is_published_as_a_live_event() {
        let (bus, handle) = channel();
        let mut sink = BusSink::new(&bus);
        sink.activity(Activity::Text("olá"));
        assert!(matches!(
            handle.recv(Duration::from_millis(50)),
            Ok(Event::Live(_))
        ));
    }

    #[test]
    fn steer_is_buffered_and_drained() {
        let (bus, handle) = channel();
        let _sent = handle.send(Command::Steer("vai".to_string()));
        let mut sink = BusSink::new(&bus);
        assert_eq!(sink.steer(), Some("vai".to_string()));
        assert_eq!(sink.steer(), None);
    }

    #[test]
    fn progress_publishes_live_tool_output() {
        let (bus, handle) = channel();
        let progress = BusProgress::new(bus.publisher());
        progress.chunk("bash", "olá\n");
        assert!(matches!(
            handle.recv(Duration::from_millis(50)),
            Ok(Event::Live(Live::ToolOutput { name, chunk })) if name == "bash" && chunk == "olá\n"
        ));
    }
}
