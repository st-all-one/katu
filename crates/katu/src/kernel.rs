//! Actor do kernel (`KERNEL_SURFACE` F1): dono do runtime e do provider, na sua própria thread.
//!
//! O kernel **não** conhece o terminal: lê [`Command`]s do canal e publica [`Event`]s. O turno corre
//! aqui; a atividade efémera e a aprovação passam pelo [`BusSink`] (request/response). A superfície
//! (TUI/CLI) é só um cliente. Ver `wiki/_ref/plan/KERNEL_SURFACE.md`.

use katu_core::api::{Command, Event, KernelBus, LoginRequest, TurnSummary};
use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::next_phase;
use katu_core::ports::Fs as _;
use katu_core::provider::{ModelSpec, Provider};
use katu_providers::PriceTable;

use crate::agent::{
    DEFAULT_IDLE_MS, Ports, SYSTEM, StepModel, TurnOptions, TurnReport, TurnRequest,
    build_provider, run_turn_with, shell_dispatch, stop_label,
};
use crate::login;
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;
use crate::tier::{PhaseModel, TierPolicy};

/// Alias local: o kernel fala em `Event` do protocolo (sem depender da UI; K3).
use katu_core::api::Event as Update;

mod control;
mod sink;
#[cfg(test)]
mod tests;
mod transcript;
mod trash;
mod usage;
mod verify;

pub(crate) use control::{models_for, thinking_options};
pub(crate) use sink::{BusProgress, BusSink};
use usage::usage_line;

/// Actor do kernel: dono do runtime, do provider e do ciclo de vida da sessão.
pub(super) struct Kernel<'a> {
    pub(super) runtime: Runtime<'a>,
    pub(super) provider: std::sync::Arc<dyn Provider>,
    pub(super) fs: &'a StdFs,
    pub(super) process: StdProcess,
    pub(super) env: StdEnv,
    pub(super) model: ModelSpec,
    /// Modelo/pensamento **fixados** pela superfície (CLI `--model`/`--thinking`); `None` deixa o
    /// runtime/`tiers` decidir (a TUI usa `SetModel`/`SetThinking` logados).
    pub(super) turn_model: Option<ModelSpec>,
    pub(super) tiers: TierPolicy,
    pub(super) prices: PriceTable,
    pub(super) max_tokens: u32,
    pub(super) max_steps: u32,
}

impl Kernel<'_> {
    /// Corre o ciclo do kernel até `Shutdown` ou até a superfície fechar (K1/K4).
    #[allow(
        clippy::needless_pass_by_value,
        reason = "a thread possui o extremo do kernel; o canal fecha quando `run` retorna"
    )]
    pub(super) fn run(mut self, bus: KernelBus) {
        let _span = katu_core::trace_fn!("tui::kernel::run");

        while let Ok(command) = bus.recv() {
            let mut sink = BusSink::new(&bus);
            match command {
                Command::Shutdown => {
                    sink.publish(Event::Done);
                    break;
                }
                Command::Cancel => bus.cancel_flag().request(),
                // Fora de um turno não há steering/pedido pendente: o sink é quem os drena.
                Command::Steer(_) | Command::Approval(_) => {}
                // S1/PI_GAINS: fora de um turno não há fim natural para adiar — consome o pedido
                // one-shot (senão ficaria pendente para o turno seguinte).
                Command::Continue => {
                    let _consumed = bus.continue_flag().take();
                }
                other => {
                    let updates = self.handle(other, &mut sink);
                    for update in updates {
                        sink.publish(update);
                    }
                    sink.publish(Event::Done);
                }
            }
        }
    }

    /// Executa um comando de trabalho e devolve as atualizações a publicar (K4).
    fn handle(&mut self, command: Command, sink: &mut BusSink<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::TUI_ACTION, "kernel::handle");

        match command {
            Command::Submit(goal) => self.submit(&goal, sink),
            Command::SetModel(model) => self.set_model(model),
            Command::SetThinking(thinking) => self.set_thinking(thinking),
            Command::Trash => self.trash_list(),
            Command::Transcript => self.transcript_view(),
            Command::Restore(token) => self.restore(&token),
            Command::EmptyTrash => self.empty_trash(sink),
            Command::Compact => self.toggle_compaction(),
            Command::Verify => self.verify(sink),
            Command::Plan => self.toggle_plan(),
            Command::Shell(command) => self.shell(&command),
            Command::Skill(name) => self.skill(&name, sink),
            Command::Login(request) => self.login(&request),
            Command::Shutdown
            | Command::Cancel
            | Command::Steer(_)
            | Command::Approval(_)
            | Command::Continue => Vec::new(),
        }
    }

    /// Liga/desliga a compactação do histórico (E09-T07/E10-T07) — comando explícito do utilizador.
    ///
    /// Liga só quando há algo a compactar; nunca compacta em silêncio (nada muda se não houver
    /// prefixo fora do orçamento). Desligar volta ao `assemble` puro.
    fn toggle_compaction(&mut self) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::toggle_compaction");

        if self.runtime.compaction() == CompactionMode::Enabled {
            self.runtime.set_compaction(CompactionMode::Disabled);
            return vec![Update::Info("compactação desligada".to_string())];
        }
        match self.runtime.compaction_preview() {
            Ok(Some(compaction)) if !compaction.replacements.is_empty() => {
                self.runtime.set_compaction(CompactionMode::Enabled);
                vec![Update::Info(format!(
                    "compactação ligada: {} → {} tokens ({} substituições)",
                    compaction.original_tokens,
                    compaction.context.tokens,
                    compaction.replacements.len()
                ))]
            }
            Ok(_) => vec![Update::Info("nada a compactar".to_string())],
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Liga/desliga o modo de planeamento (E20-T11) e, ao ligar, escreve o esqueleto do plano.
    fn toggle_plan(&mut self) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::toggle_plan");

        let on = !self.runtime.plan_mode();
        match self.runtime.set_plan_mode(on) {
            Ok(state) => {
                let mut updates = vec![Update::Plan(state)];
                if state {
                    match self.runtime.write_plan_artifact(self.fs) {
                        Ok(path) => {
                            updates.push(Update::Info(format!("plano: {}", path.display())));
                        }
                        Err(error) => updates.push(Update::Error(format!("plano: {error}"))),
                    }
                }
                updates
            }
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Faz login num provider (E21): escreve a seleção/credenciais e reconstrói o provider.
    ///
    /// O embedding não é tocado: o login só muda o agente principal. Depois de reconstruir, o
    /// modelo passa a ser controlo logado e a lista de modelos é republicada.
    fn login(&mut self, request: &LoginRequest) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::login");

        let intent = if request.logout {
            login::Intent::Logout
        } else {
            login::Intent::Login
        };
        let resolved = match login::resolve(
            Some(&request.provider),
            request.api_key.as_deref(),
            request.model.as_deref(),
            request.base.as_deref(),
            intent,
        ) {
            Ok(resolved) => resolved,
            Err(error) => return vec![Update::Error(error.to_string())],
        };
        let outcome = match login::apply(&resolved) {
            Ok(outcome) => outcome,
            Err(error) => return vec![Update::Error(error.to_string())],
        };
        if request.logout {
            return vec![Update::Info("sessão terminada".to_string())];
        }
        let provider = match build_provider(
            outcome.provider,
            &outcome.base,
            &self.env,
            self.runtime.session_id(),
        ) {
            Ok(provider) => provider,
            Err(message) => return vec![Update::Error(message)],
        };
        self.provider = std::sync::Arc::from(provider);
        let models = models_for(self.provider.as_ref(), &outcome.model);
        let mut updates = Vec::new();
        if let Some(warning) = &outcome.warning {
            updates.push(Update::Info(warning.clone()));
        }
        updates.push(Update::Info(format!(
            "login: {} ({})",
            outcome.provider, outcome.model
        )));
        updates.push(Update::Models(models));
        updates.extend(self.set_model(outcome.model.clone()));
        if let Some(message) = login::verify(&outcome) {
            updates.push(Update::Info(message));
        }
        updates
    }

    /// Executa `!<cmd>` pela política/contenção (E20-T12).
    fn shell(&mut self, command: &str) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::shell");

        let ports = Ports {
            fs: self.fs,
            process: &self.process,
            env: &self.env,
        };
        match shell_dispatch(&mut self.runtime, &ports, command) {
            Ok(dispatch) => shell_updates(&dispatch),
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Força o carregamento de uma skill pelo nome (E20-T13): o `SKILL.md` vira objetivo do turno.
    fn skill(&mut self, name: &str, sink: &mut BusSink<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::SKILL_READ, "kernel::skill");
        let Some(skill) = self.runtime.skill(name) else {
            return vec![Update::Error(format!("skill desconhecida: {name}"))];
        };
        let path = skill.path.clone();
        match self.fs.read(&path) {
            Ok(bytes) => self.submit(&String::from_utf8_lossy(&bytes), sink),
            Err(error) => vec![Update::Error(format!("skill {name}: {error}"))],
        }
    }

    /// Submete um turno e traduz o resultado em atualizações da UI.
    #[allow(
        clippy::too_many_lines,
        reason = "a submissão (modelo por passo, portas, progresso, envelope) é um fluxo único; dividi-lo esconderia a ordem"
    )]
    fn submit(&mut self, goal: &str, sink: &mut BusSink<'_>) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::submit");

        // `P1/PI_GAINS`: o output incremental das tools publica-se **diretamente** no canal (é
        // efémero e pode vir de threads paralelas), sem passar pelo `ActivitySink` do turno.
        let progress = BusProgress::new(sink.publisher());
        let ports = Ports {
            fs: self.fs,
            process: &self.process,
            env: &self.env,
        };
        let control = self.runtime.control();
        let pinned = self.turn_model.is_some() || control.model.is_some();
        let model = self.turn_model.clone().unwrap_or_else(|| ModelSpec {
            model: control.model.clone().unwrap_or_else(|| {
                self.tiers.model_for(
                    self.provider.as_ref(),
                    self.runtime.phase(),
                    &self.model.model,
                )
            }),
            thinking: control.thinking,
        });
        // `Q2/PI_GAINS`: sem modelo fixado pelo utilizador, o loop re-resolve o modelo **por passo**
        // a partir da fase; com modelo fixado não há resolver (o explícito vence, E12-T10).
        let step_model: Option<Box<dyn StepModel>> = if pinned {
            None
        } else {
            let resolver: Box<dyn StepModel> = Box::new(PhaseModel::new(
                std::sync::Arc::clone(&self.provider),
                self.tiers.clone(),
                self.model.model.clone(),
                control.thinking,
            ));
            Some(resolver)
        };
        let options = TurnOptions {
            model,
            system: Some(SYSTEM.to_string()),
            max_tokens: self.max_tokens,
            temperature: 0.0,
            max_steps: self.max_steps,
            idle_ms: DEFAULT_IDLE_MS,
            step_model,
        };
        // K8: a flag de cancelamento é a partilhada com a superfície (Esc/Ctrl-C) e as tools.
        let cancel = sink.cancel_flag();
        match run_turn_with(
            &mut self.runtime,
            TurnRequest {
                provider: std::sync::Arc::clone(&self.provider),
                ports,
                goal,
                options: &options,
                cancel: Some(&cancel),
                progress: &progress,
            },
            sink,
        ) {
            Ok(turn) => self.turn_updates(turn),
            Err(error) => {
                let error: Error = error.into();
                vec![Update::Failure {
                    kind: error.kind(),
                    message: error.to_string(),
                }]
            }
        }
    }

    /// Traduz o fim do turno em atualizações (uso, cancelamento, fase, checkpoint, transcript).
    ///
    /// O `Done` **não** é emitido aqui: o ciclo do kernel fecha cada comando com ele (K4).
    fn turn_updates(&self, turn: TurnReport) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::kernel::turn_updates");

        // Envelope de máquina: o CLI usa-o; a TUI ignora-o (já recebe `Assistant`/`Usage`).
        let summary = TurnSummary {
            model: turn.model.clone(),
            text: turn.text.clone(),
            steps: turn.steps,
            calls: turn.calls,
            cancelled: turn.cancelled,
            stop: stop_label(&turn.stop).to_string(),
            termination: turn.termination.as_str().to_string(),
            round_exit: turn.termination.round_exit(),
            usage: turn.usage,
            session: self.runtime.session_id().map(str::to_string),
            state: self.runtime.state_text().map(str::to_string),
            selection: self.runtime.selection().as_str().to_string(),
        };
        let usage = usage_line(&turn.model, &turn, &self.prices);
        let cancelled = turn.cancelled;
        let calls = turn.calls;
        // Turno sem texto e sem cancelamento: quase sempre o modelo gastou o orçamento de saída em
        // raciocínio (`max_completion_tokens`); o utilizador não pode ficar sem qualquer sinal.
        let empty = turn.text.trim().is_empty() && !cancelled;
        let mut updates = if empty {
            vec![Update::Error(
                "o modelo não devolveu texto (resposta vazia ou só raciocínio); aumenta \
                 `--max-tokens` ou muda de modelo"
                    .to_string(),
            )]
        } else {
            vec![Update::Assistant(turn.text)]
        };
        if let Some(usage) = usage {
            updates.push(Update::Usage(usage));
        }
        if cancelled {
            updates.push(Update::Cancelled);
        }
        if calls > 0 {
            updates.push(Update::Info(format!("{calls} tool call(s)")));
        }
        updates.push(Update::Phase(self.runtime.phase().as_str().to_string()));
        let next = next_phase(self.runtime.phase()).map_or_else(
            || "concluído".to_string(),
            |phase| phase.as_str().to_string(),
        );
        if let Err(error) = self.runtime.write_checkpoint(&next) {
            updates.push(Update::Error(error.to_string()));
        }
        updates.push(Update::NextAction(next));
        if let Some(error) = self.write_transcript() {
            updates.push(error);
        }
        updates.push(Update::Turn(Box::new(summary)));
        updates
    }
}

mod shell;

use shell::shell_updates;
