//! Executor do loop de turnos para a TUI (E10-T05/T10): dono do runtime e do provider.
//!
//! Vive num módulo filho para manter `tui.rs` sob o teto de linhas. Durante o turno, os deltas e as
//! tools em curso são reencaminhados **ao vivo** para o painel de atividade (E10-T05) através do
//! `Painter`, sem entrarem no log nem no transcript. O utilizador pode cancelar (Esc/Ctrl-C) e ver
//! o uso/custo do turno (E12-T03/T10).

use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::kernel::next_phase;
use katu_core::provider::{ModelSpec, Provider};
use katu_providers::PriceTable;
use katu_tui::{ChallengePrompt, Command, Handler, Live, Painter, Update};

use crate::agent::{
    Activity, ActivitySink, Approval, ApprovalPrompt, Ports, SYSTEM, TurnOptions, TurnReport,
    TurnRequest, run_turn_with,
};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;
use crate::tier::TierPolicy;

/// Executor do loop de turnos para a UI (dono do runtime e do provider).
pub(super) struct AgentHandler<'a> {
    pub(super) runtime: Runtime<'a>,
    pub(super) provider: Box<dyn Provider>,
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
        match command {
            Command::Quit => Vec::new(),
            Command::Submit(goal) => self.submit(&goal, painter),
            Command::SetModel(model) => self.set_model(model),
            Command::SetThinking(thinking) => self.set_thinking(thinking),
            Command::Trash => self.trash_list(),
            Command::Transcript => self.transcript_view(),
            Command::Restore(token) => self.restore(&token),
            Command::EmptyTrash => self.empty_trash(painter),
            Command::Compact => self.toggle_compaction(),
            Command::Verify => self.verify(painter),
        }
    }
}

impl AgentHandler<'_> {
    /// Liga/desliga a compactação do histórico (E09-T07/E10-T07) — comando explícito do utilizador.
    ///
    /// Liga só quando há algo a compactar; nunca compacta em silêncio (nada muda se não houver
    /// prefixo fora do orçamento). Desligar volta ao `assemble` puro.
    fn toggle_compaction(&mut self) -> Vec<Update> {
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

    /// Submete um turno e traduz o resultado em atualizações da UI.
    fn submit(&mut self, goal: &str, painter: &mut Painter<'_>) -> Vec<Update> {
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
        };
        let mut activity = LivePainter {
            painter,
            granted_by,
        };
        match run_turn_with(
            &mut self.runtime,
            TurnRequest {
                provider: self.provider.as_ref(),
                ports,
                goal,
                options: &options,
            },
            &mut activity,
        ) {
            Ok(turn) => self.turn_updates(&options.model.model, turn),
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Traduz o fim do turno em atualizações (uso, cancelamento, fase, checkpoint, transcript).
    fn turn_updates(&self, model: &str, turn: TurnReport) -> Vec<Update> {
        let usage = usage_line(model, &turn, &self.prices);
        let cancelled = turn.cancelled;
        let calls = turn.calls;
        let mut updates = vec![Update::Assistant(turn.text)];
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

/// Linha de uso/custo do turno (E12-T03), com a base de evidência. `None` sem contabilização.
///
/// O custo só aparece quando o modelo tem preço em `policy/prices.toml` (nunca inventado, DF5).
fn usage_line(model: &str, turn: &TurnReport, prices: &PriceTable) -> Option<String> {
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

/// Adapta o [`Painter`] da UI ao observador efémero do loop (E10-T05) e ao challenge de aprovação
/// (E10-T04). O cancelamento (Esc/Ctrl-C) chega pelo mesmo `Painter`.
struct LivePainter<'p, 'a> {
    painter: &'p mut Painter<'a>,
    granted_by: String,
}

impl ActivitySink for LivePainter<'_, '_> {
    fn cancelled(&self) -> bool {
        self.painter.cancelled()
    }

    fn approve(&mut self, prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        let request = ChallengePrompt {
            tool: prompt.tool.to_string(),
            rule: prompt.request.rule_id.as_str().to_string(),
            scope: prompt.request.scope.clone(),
        };
        let signature = self.painter.challenge(request, &self.granted_by)?;
        katu_core::event!(
            Level::Warn,
            events::TUI_APPROVAL,
            "tool" => prompt.tool,
            "rule" => prompt.request.rule_id.as_str()
        );
        Some(Approval {
            reason: signature.reason,
            granted_by: signature.granted_by,
        })
    }

    fn activity(&mut self, activity: Activity<'_>) {
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
            Activity::ToolDone { name } => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool_done");
                Live::ToolDone(name.to_string())
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
        self.painter.live(live);
    }
}

#[cfg(test)]
mod tests {
    use katu_core::evidence::EvidenceBasis;
    use katu_core::provider::TokenUsage;
    use katu_providers::{Price, PriceTable};

    use super::usage_line;
    use crate::agent::TurnReport;

    fn report(usage: Option<TokenUsage>) -> TurnReport {
        TurnReport {
            steps: 1,
            text: String::new(),
            calls: 0,
            usage,
            cancelled: false,
        }
    }

    #[test]
    fn usage_line_shows_tokens_and_cost_when_priced() {
        let mut usage = TokenUsage::new(EvidenceBasis::ProviderReported);
        usage.input = Some(1_000);
        usage.output = Some(500);
        let mut prices = PriceTable::new();
        prices.set(
            "m",
            Price {
                input: 1_000_000,
                output: 3_000_000,
                cached_input: 0,
            },
        );
        let line = usage_line("m", &report(Some(usage)), &prices).unwrap_or_default();
        assert!(line.contains("in 1000"), "{line}");
        assert!(line.contains("out 500"), "{line}");
        assert!(line.contains("custo"), "{line}");
    }

    #[test]
    fn usage_line_without_usage_is_none() {
        assert!(usage_line("m", &report(None), &PriceTable::new()).is_none());
    }
}
