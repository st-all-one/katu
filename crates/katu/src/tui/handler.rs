//! Executor do loop de turnos para a TUI (E10-T05/T10): dono do runtime e do provider.
//!
//! Vive num módulo filho para manter `tui.rs` sob o teto de linhas. Durante o turno, os deltas e as
//! tools em curso são reencaminhados **ao vivo** para o painel de atividade (E10-T05) através do
//! `Painter`, sem entrarem no log nem no transcript. O utilizador pode cancelar (Esc/Ctrl-C) e ver
//! o uso/custo do turno (E12-T03/T10).

use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::Dispatch;
use katu_core::kernel::next_phase;
use katu_core::ports::Fs as _;
use katu_core::provider::{ModelSpec, Provider};
use katu_core::report::ToolReport;
use katu_providers::PriceTable;
use katu_tui::{Command, Handler, Painter, Update};

use crate::agent::{
    DEFAULT_IDLE_MS, Ports, SYSTEM, TurnOptions, TurnReport, TurnRequest, build_provider,
    run_turn_with, shell_dispatch,
};
use crate::login;
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;
use crate::tier::TierPolicy;

mod live;

use live::LivePainter;

/// Executor do loop de turnos para a UI (dono do runtime e do provider).
pub(super) struct AgentHandler<'a> {
    pub(super) runtime: Runtime<'a>,
    pub(super) provider: std::sync::Arc<dyn Provider>,
    pub(super) fs: &'a StdFs,
    pub(super) process: StdProcess,
    pub(super) env: StdEnv,
    pub(super) model: ModelSpec,
    pub(super) tiers: TierPolicy,
    pub(super) prices: PriceTable,
    pub(super) max_tokens: u32,
    pub(super) max_steps: u32,
}

impl Handler for AgentHandler<'_> {
    fn handle(&mut self, command: Command, painter: &mut Painter<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::TUI_ACTION, "handler::handle");
        match command {
            // K8/F1: `Shutdown`/`Cancel`/`Steer`/`Approval` são do actor (o cancelamento durante o
            // turno é a flag partilhada que o `Painter` escreve no `Esc`/`Ctrl-C`); inertes aqui.
            Command::Shutdown | Command::Cancel | Command::Steer(_) | Command::Approval(_) => {
                Vec::new()
            }
            Command::Submit(goal) => self.submit(&goal, painter),
            Command::SetModel(model) => self.set_model(model),
            Command::SetThinking(thinking) => self.set_thinking(thinking),
            Command::Trash => self.trash_list(),
            Command::Transcript => self.transcript_view(),
            Command::Restore(token) => self.restore(&token),
            Command::EmptyTrash => self.empty_trash(painter),
            Command::Compact => self.toggle_compaction(),
            Command::Verify => self.verify(painter),
            Command::Plan => self.toggle_plan(),
            Command::Shell(command) => self.shell(&command),
            Command::Skill(name) => self.skill(&name, painter),
            Command::Login(request) => self.login(&request),
        }
    }
}

impl AgentHandler<'_> {
    /// Liga/desliga a compactação do histórico (E09-T07/E10-T07) — comando explícito do utilizador.
    ///
    /// Liga só quando há algo a compactar; nunca compacta em silêncio (nada muda se não houver
    /// prefixo fora do orçamento). Desligar volta ao `assemble` puro.
    fn toggle_compaction(&mut self) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::toggle_compaction");

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
        let _span = katu_core::trace_fn!("tui::handler::toggle_plan");

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
    fn login(&mut self, request: &katu_tui::LoginRequest) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::login");

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
        let models = super::models_for(self.provider.as_ref(), &outcome.model);
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
        let _span = katu_core::trace_fn!("tui::handler::shell");

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
    fn skill(&mut self, name: &str, painter: &mut Painter<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::SKILL_READ, "handler::skill");
        let Some(skill) = self.runtime.skill(name) else {
            return vec![Update::Error(format!("skill desconhecida: {name}"))];
        };
        let path = skill.path.clone();
        match self.fs.read(&path) {
            Ok(bytes) => self.submit(&String::from_utf8_lossy(&bytes), painter),
            Err(error) => vec![Update::Error(format!("skill {name}: {error}"))],
        }
    }

    /// Submete um turno e traduz o resultado em atualizações da UI.
    fn submit(&mut self, goal: &str, painter: &mut Painter<'_>) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::submit");

        let granted_by = self.granted_by();
        let ports = Ports {
            fs: self.fs,
            process: &self.process,
            env: &self.env,
        };
        let control = self.runtime.control();
        let model = ModelSpec {
            model: control.model.clone().unwrap_or_else(|| {
                self.tiers.model_for(
                    self.provider.as_ref(),
                    self.runtime.phase(),
                    &self.model.model,
                )
            }),
            thinking: control.thinking,
        };
        let options = TurnOptions {
            model,
            system: Some(SYSTEM.to_string()),
            max_tokens: self.max_tokens,
            temperature: 0.0,
            max_steps: self.max_steps,
            idle_ms: DEFAULT_IDLE_MS,
        };
        // L-P1/L-P3: a flag de cancelamento é partilhada com a UI (Esc/Ctrl-C) e com as tools
        // (o `bash` longo é interrompido). A UI sonda o input a cada `tick` do loop.
        let cancel = painter.cancel_flag();
        let mut activity = LivePainter {
            painter,
            granted_by,
        };
        match run_turn_with(
            &mut self.runtime,
            TurnRequest {
                provider: std::sync::Arc::clone(&self.provider),
                ports,
                goal,
                options: &options,
                cancel: Some(&cancel),
            },
            &mut activity,
        ) {
            Ok(turn) => self.turn_updates(&options.model.model, turn),
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Traduz o fim do turno em atualizações (uso, cancelamento, fase, checkpoint, transcript).
    fn turn_updates(&self, model: &str, turn: TurnReport) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::handler::turn_updates");

        let usage = usage_line(model, &turn, &self.prices);
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
        updates.push(Update::Done);
        updates
    }
}

/// Traduz o `Dispatch` de um `!<cmd>` em atualizações (recusa com evidência ou saída).
fn shell_updates(dispatch: &Dispatch) -> Vec<Update> {
    let _span = katu_core::trace_fn!("tui::handler::shell_updates");

    match dispatch.outcome() {
        ToolOutcome::Denied { rule_id, evidence } => vec![Update::Error(format!(
            "negado por {}: {}",
            rule_id.as_str(),
            evidence.argument
        ))],
        ToolOutcome::Unavailable { control, .. } => {
            vec![Update::Error(format!(
                "! indisponível: {}",
                control.as_str()
            ))]
        }
        _ => {
            let body = dispatch
                .report()
                .map(ToolReport::to_toon)
                .unwrap_or_default();
            vec![Update::Info(body), Update::Done]
        }
    }
}

/// Linha de uso/custo do turno (E12-T03), com a base de evidência. `None` sem contabilização.
///
/// O custo só aparece quando o modelo tem preço em `policy/prices.toml` (nunca inventado, DF5).
fn usage_line(model: &str, turn: &TurnReport, prices: &PriceTable) -> Option<String> {
    let _span = katu_core::trace_fn!("tui::handler::usage_line");

    let usage = turn.usage.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    for (label, tokens) in [
        ("in", usage.input),
        ("out", usage.output),
        ("cache", usage.cached_input),
        ("think", usage.reasoning),
    ] {
        if let Some(count) = tokens {
            parts.push(format!("{label} {count}"));
        }
    }
    if let Some(micros) = prices.cost(model, usage).micros {
        parts.push(format!("custo {micros} µUS$"));
    }
    if parts.is_empty() {
        return Some(format!("tokens n/d ({})", usage.basis.as_str()));
    }
    Some(format!("tokens {}", parts.join(" ")))
}

#[cfg(test)]
mod tests;
