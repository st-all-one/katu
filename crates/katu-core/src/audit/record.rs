//! Registo de auditoria (ADR 0009): projeção **densa** de um evento do log.
//!
//! Cada evento vira uma linha da tabela `a` (`seq,kind,tool,path,status,rule,text`). O `text` é um
//! excerto limitado (o corpo integral continua no log); a pesquisa indexa `text`/`path`/`tool`/
//! `status`/`rule`/`kind`.

use crate::kernel::{Control, Event};
use crate::toon::{Cell, RowTable};

/// Bytes máximos do excerto pesquisável.
pub const MAX_TEXT_BYTES: usize = 512;

/// Linha de auditoria derivada de um evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// `seq` do log (endereço estável).
    pub seq: u64,
    /// Tipo do evento (domínio fechado).
    pub kind: &'static str,
    /// Nome da tool, quando aplicável.
    pub tool: String,
    /// Caminho principal, quando aplicável.
    pub path: String,
    /// Estado (outcome/exit/fase), quando aplicável.
    pub status: String,
    /// Regra de política, quando aplicável.
    pub rule: String,
    /// Excerto pesquisável.
    pub text: String,
}

impl AuditRecord {
    /// Projeta um evento do log numa linha de auditoria.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "projeção exaustiva de todos os eventos num só sítio (tabela de equivalência)"
    )]
    pub fn from_event(seq: u64, event: &Event) -> Self {
        let mut record = Self {
            seq,
            kind: "event",
            tool: String::new(),
            path: String::new(),
            status: String::new(),
            rule: String::new(),
            text: String::new(),
        };
        match event {
            Event::TurnStart { turn } => {
                record.kind = "turn";
                record.text = format!("turn {turn} start");
            }
            Event::TurnEnd { turn } => {
                record.kind = "turn";
                record.text = format!("turn {turn} end");
            }
            Event::UserMessage { text } => {
                record.kind = "user";
                record.text.clone_from(text);
            }
            Event::AssistantMessage { text } => {
                record.kind = "assistant";
                record.text.clone_from(text);
            }
            Event::ToolCall { tool, .. } => {
                record.kind = "tool_call";
                record.tool = tool.name.as_str().to_string();
                if let Some(path) = tool.resolved_paths.first() {
                    record.path = path.as_str().to_string();
                }
                record.text.clone_from(&record.path);
            }
            Event::ToolResult { outcome, .. } => {
                record.kind = "tool_result";
                record.status = outcome.status_str().to_string();
                if let Some(rule) = outcome.rule_id() {
                    record.rule = rule.as_str().to_string();
                }
                record.text = outcome.summary();
            }
            Event::PhaseTransition { to, .. } => {
                record.kind = "phase";
                record.status = to.as_str().to_string();
                record.text = format!("phase {}", to.as_str());
            }
            Event::Waiver { transition, reason } => {
                record.kind = "waiver";
                record.status = transition.as_str().to_string();
                record.text.clone_from(reason);
            }
            Event::PlanRecorded { plan } => {
                record.kind = "plan";
                record.text = plan
                    .feature_list
                    .iter()
                    .map(|feature| format!("{} {}", feature.id, feature.description))
                    .collect::<Vec<_>>()
                    .join("; ");
            }
            Event::CommandRecorded { record: command } => {
                record.kind = "command";
                record.status = command
                    .exit_code
                    .map_or_else(|| "signal".to_string(), |code| code.to_string());
                record.path.clone_from(&command.cwd);
                record.text = command.argv.join(" ");
            }
            Event::WorkspaceSet { root } => {
                record.kind = "workspace";
                record.path = root.as_str().to_string();
                record.text = "workspace set".to_string();
            }
            Event::ApprovalGranted {
                rule_id,
                granted_by,
                ..
            } => {
                record.kind = "approval";
                record.rule = rule_id.as_str().to_string();
                record.text = format!("aprovado por {granted_by}");
            }
            Event::VerificationRecorded { report } => {
                record.kind = "verify";
                record.status = report.status.as_str().to_string();
                record.text = report
                    .checks
                    .iter()
                    .map(|check| format!("{}:{}", check.id, check.status.as_str()))
                    .collect::<Vec<_>>()
                    .join("; ");
            }
            Event::Control { control } => {
                record.kind = "control";
                record.text = match control {
                    Control::SetModel { model } => format!("model {model}"),
                    Control::SetThinking { thinking } => {
                        format!("thinking {}", thinking.as_str())
                    }
                };
            }
            Event::ProjectContext { agents, skills } => {
                record.kind = "context";
                record.text = format!(
                    "agents {} B; skills {} B",
                    agents.as_deref().map_or(0, str::len),
                    skills.as_deref().map_or(0, str::len)
                );
            }
        }
        record.text = truncate(&record.text, MAX_TEXT_BYTES);
        record
    }

    /// Campos pesquisáveis por ordem de `field_id`.
    #[must_use]
    pub fn fields(&self) -> [&str; 6] {
        let _span = crate::trace_fn!("audit::record::fields");

        [
            &self.text,
            &self.path,
            &self.tool,
            &self.status,
            &self.rule,
            self.kind,
        ]
    }

    /// Linha da tabela `a`.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        let _span = crate::trace_fn!("audit::record::cells");

        vec![
            Cell::int(i64::try_from(self.seq).unwrap_or(i64::MAX)),
            Cell::text(self.kind),
            Cell::text(self.tool.clone()),
            Cell::text(self.path.clone()),
            Cell::text(self.status.clone()),
            Cell::text(self.rule.clone()),
            Cell::text(self.text.clone()),
        ]
    }
}

/// Trunca num limite de carácter válido.
fn truncate(text: &str, max_bytes: usize) -> String {
    let _span = crate::trace_fn!("audit::record::truncate");

    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    text.get(..end).unwrap_or_default().to_string()
}

/// Constrói a tabela `a` a partir de linhas de auditoria.
pub(super) fn table(records: &[AuditRecord]) -> RowTable {
    let _span = crate::trace_fn!("audit::record::table");

    let mut table = RowTable::new("a");
    for record in records {
        table.push(record.cells());
    }
    table
}

#[cfg(test)]
mod tests;
