//! Adaptador do `Painter` da UI ao observador efémero e ao challenge de aprovação (E10-T05/T04).

use katu_core::diag::{Level, events};
use katu_tui::{ChallengePrompt, Live, Painter};

use crate::agent::{Activity, ActivitySink, Approval, ApprovalPrompt};

pub(super) struct LivePainter<'p, 'a> {
    pub(super) painter: &'p mut Painter<'a>,
    pub(super) granted_by: String,
}

impl ActivitySink for LivePainter<'_, '_> {
    fn cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("tui::handler::live::cancelled");

        self.painter.cancelled()
    }

    fn steer(&mut self) -> Option<String> {
        let _span = katu_core::trace_fn!("tui::handler::live::steer");

        self.painter.take_steer()
    }

    fn approve(&mut self, prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        let _span = katu_core::trace_fn!("tui::handler::live::approve");

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
        let _span = katu_core::trace_fn!("tui::handler::live::activity");

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
