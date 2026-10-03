//! Painel de atividade **efémero** (E10-T05): só estado de UI; nunca toca no log nem no transcript.

use katu_core::diag::{Level, events};

use crate::entry::Role;
use crate::live::trim_tail;
use katu_core::api::Live;

use super::App;

/// Teto do buffer efémero de streaming (bytes); o painel mostra só a cauda (E10-T03).
const STREAM_TAIL_BYTES: usize = 8 * 1024;

impl App {
    /// Aplica um evento efémero ao painel de atividade (não toca no transcript).
    pub(super) fn apply_live(&mut self, live: Live) {
        let _span = katu_core::fn_span!(Level::Trace, events::TUI_LIVE, "app::apply_live");
        match live {
            Live::Text(delta) => {
                self.streaming.push_str(&delta);
                trim_tail(&mut self.streaming, STREAM_TAIL_BYTES);
            }
            Live::Thinking(delta) => self.thinking.push_str(&delta),
            Live::Tool { name, args } => {
                if args.is_empty() || args == "{}" {
                    self.live.push(format!("→ {name}"));
                } else {
                    self.live.push(format!("→ {name} {args}"));
                }
            }
            Live::ToolDone(name) => self.live.push(format!("✓ {name}")),
            Live::Refused { rule, evidence } => {
                let text = format!("⛔ {rule}: {evidence}");
                self.live.push(text.clone());
                self.push(Role::Error, text);
            }
            Live::Unavailable { control } => {
                self.live.push(format!("⚠ falta {control}"));
            }
            Live::Clear => self.clear_live(),
        }
    }

    /// Limpa o painel de atividade.
    pub(super) fn clear_live(&mut self) {
        let _span = katu_core::trace_fn!("app::panel::clear_live");

        self.streaming.clear();
        self.thinking.clear();
        self.live.clear();
    }
}
