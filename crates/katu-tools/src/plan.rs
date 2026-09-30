//! Tool `plan` (E06-T06): valida e reporta um **plano tipado**.
//!
//! A validação é de schema ([`Plan::validate`]): sem `forbidden_files`, sem `rollback_plan` ou com
//! mais de uma feature `in_progress`, o plano **não** é aceite (fail-closed, DF10). Quem transita a
//! fase é o kernel (`Task → … → Planned` exige um plano registado; E04).

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::plan::{FeatureStatus, Plan, PlanError};
use katu_core::report::{ToolReport, content_hash, content_id};
use katu_core::toon::Value;
use katu_policy::{ControlId, ToolName, ToolUse};

use crate::lang::{len_u64, to_i64};

/// Executor de validação de plano.
pub struct PlanTool {
    /// Plano a validar.
    pub plan: Plan,
}

impl Tool for PlanTool {
    fn name(&self) -> ToolName {
        let _span = katu_core::trace_fn!("plan::name");

        ToolName::Plan
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_PLAN, "plan::execute");
        match self.plan.validate() {
            Ok(()) => ToolOutput::report(build(&self.plan)),
            Err(error) => unavailable(control_of(&error)),
        }
    }
}

/// Controlo acionável correspondente ao erro (DF10).
fn control_of(error: &PlanError) -> &'static str {
    let _span = katu_core::trace_fn!("plan::control_of");

    match error {
        PlanError::EmptyFeatureList => "plan-features",
        PlanError::MissingForbiddenFiles => "plan-forbidden",
        PlanError::MissingRollbackPlan => "plan-rollback",
        PlanError::MultipleInProgress => "plan-in-progress",
        _ => "plan",
    }
}

fn build(plan: &Plan) -> ToolReport {
    let _span = katu_core::fn_span!(Level::Trace, events::TOOL_PLAN, "plan::build");
    let features: Vec<Value> = plan
        .feature_list
        .iter()
        .map(|feature| {
            Value::map(vec![
                ("id".to_string(), Value::str(feature.id.clone())),
                ("status".to_string(), Value::str(feature.status.as_str())),
                ("desc".to_string(), Value::str(feature.description.clone())),
            ])
        })
        .collect();
    let in_progress = plan
        .feature_list
        .iter()
        .filter(|feature| feature.status == FeatureStatus::InProgress)
        .count();
    let forbidden: Vec<Value> = plan
        .scope_contract
        .forbidden_files
        .iter()
        .map(|glob| Value::str(glob.clone()))
        .collect();
    let allowed: Vec<Value> = plan
        .scope_contract
        .allowed_files
        .iter()
        .map(|glob| Value::str(glob.clone()))
        .collect();
    let data = Value::map(vec![
        ("features".to_string(), Value::list(features)),
        (
            "in_progress".to_string(),
            Value::int(to_i64(len_u64(in_progress))),
        ),
        ("forbidden_files".to_string(), Value::list(forbidden)),
        ("allowed_files".to_string(), Value::list(allowed)),
        (
            "rollback".to_string(),
            Value::str(plan.scope_contract.rollback_plan.clone()),
        ),
    ]);
    let seed = plan
        .feature_list
        .iter()
        .map(|feature| feature.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    ToolReport::new("plan.validate", data)
        .with_id(content_id("p", seed.as_bytes()))
        .with_hash(content_hash(seed.as_bytes()))
}

fn unavailable(control: &'static str) -> ToolOutput {
    let _span = katu_core::trace_fn!("plan::unavailable");

    ToolOutput::outcome(ToolOutcome::Unavailable {
        control: ControlId::new(control),
        rule_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::PlanTool;
    use katu_core::error::ToolOutcome;
    use katu_core::kernel::{Tool, ToolOutput};
    use katu_core::plan::{Feature, FeatureStatus, Plan, ScopeContract};
    use katu_core::report::ToolReport;
    use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

    fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical("/work")?;
        Ok(ToolUse {
            name: ToolName::Plan,
            args: ToolArgs::Plan,
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    fn plan(in_progress: usize) -> Plan {
        let mut features: Vec<Feature> = Vec::new();
        for index in 0..in_progress {
            features.push(Feature::new(
                format!("F{index}"),
                "fazer",
                FeatureStatus::InProgress,
            ));
        }
        if features.is_empty() {
            features.push(Feature::new("F1", "fazer", FeatureStatus::Pending));
        }
        Plan::new(
            ScopeContract::new(
                vec!["src/**".to_string()],
                vec!["**/secrets/**".to_string()],
                vec!["testes passam".to_string()],
                "reverter o commit",
            ),
            features,
        )
    }

    fn render(output: &ToolOutput) -> String {
        output
            .report
            .as_ref()
            .map_or_else(String::new, ToolReport::to_toon)
    }

    #[test]
    fn valid_plan_reports() -> Result<(), Box<dyn std::error::Error>> {
        let output = PlanTool { plan: plan(1) }.execute(&use_()?);
        assert_eq!(output.outcome, ToolOutcome::Ok);
        let rendered = render(&output);
        assert!(rendered.contains("plan.validate\u{1f}"), "{rendered}");
        assert!(rendered.contains("in_progress\u{1f}1\n"), "{rendered}");
        assert!(rendered.contains("\u{1e}features\n"), "{rendered}");
        Ok(())
    }

    #[test]
    fn missing_forbidden_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let mut plan = plan(1);
        plan.scope_contract.forbidden_files.clear();
        let output = PlanTool { plan }.execute(&use_()?);
        assert!(matches!(
            output.outcome,
            ToolOutcome::Unavailable { control, .. } if control.as_str() == "plan-forbidden"
        ));
        Ok(())
    }

    #[test]
    fn multiple_in_progress_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let output = PlanTool { plan: plan(2) }.execute(&use_()?);
        assert!(matches!(
            output.outcome,
            ToolOutcome::Unavailable { control, .. } if control.as_str() == "plan-in-progress"
        ));
        Ok(())
    }
}
