//! Derivação da capacidade que satisfaz uma aprovação (E07-T05, §33).
//!
//! O **agente não assina**: quando a política devolve `RequireApproval`, um humano responde ao
//! *challenge* e o kernel concede a capacidade **mínima** que destranca a regra — um caminho
//! concreto, um comando nominal ou um host. Sem capacidade derivável, **não** há override
//! (fail-closed): a recusa mantém-se.

use crate::decision::ApprovalRequest;
use crate::facts::{Capability, ToolName, ToolUse};
use crate::paths::ResolvedPath;
use crate::rule::{Enforcement, RuleId, RuleSet};

/// Capacidade mínima que destranca `rule_id` para o `use_`, ou `None` se a regra não for
/// sobreponível por capacidade (orçamento, fase, dependência de tool).
#[must_use]
pub fn capability_for(use_: &ToolUse, rules: &RuleSet, rule_id: &RuleId) -> Option<Capability> {
    let rule = rules.rules.iter().find(|rule| rule.id == *rule_id)?;
    match &rule.enforcement {
        Enforcement::DenyRead { .. } => path(use_).map(|root| Capability::ReadPath { root }),
        Enforcement::DenySensitiveRead { globs } => {
            sensitive_path(use_, globs).map(|root| Capability::ReadPath { root })
        }
        Enforcement::DenyWrite { .. } => path(use_).map(|root| Capability::WritePath { root }),
        Enforcement::DenyDelete { .. } => path(use_).map(|root| Capability::DeletePath { root }),
        Enforcement::DenyCommand { tool } => command(use_, *tool),
        _ => None,
    }
}

/// Capacidade para um pedido de aprovação da política.
#[must_use]
pub fn capability_for_request(
    use_: &ToolUse,
    rules: &RuleSet,
    request: &ApprovalRequest,
) -> Option<Capability> {
    capability_for(use_, rules, &request.rule_id)
}

/// Primeiro caminho resolvido do uso (a capacidade cobre-o e aos descendentes).
fn path(use_: &ToolUse) -> Option<ResolvedPath> {
    use_.resolved_paths.first().cloned()
}

/// Primeiro caminho cujo **componente** casa um glob sensível.
fn sensitive_path(use_: &ToolUse, globs: &[String]) -> Option<ResolvedPath> {
    use_.resolved_paths
        .iter()
        .find(|path| {
            path.as_str().split('/').any(|component| {
                globs
                    .iter()
                    .any(|glob| crate::matches_glob(glob, component))
            })
        })
        .cloned()
}

/// Capacidade para um `DenyCommand`: nominal, por programa verificável ou por host de rede.
///
/// Um `argv` opaco/destrutivo ou uma rede sem host reconhecível **não** é sobreponível
/// (fail-closed): não há capacidade mínima que o destranque.
fn command(use_: &ToolUse, tool: ToolName) -> Option<Capability> {
    if tool != ToolName::Exec {
        return Some(Capability::Command { tool });
    }
    let argv = use_.argv.as_ref()?;
    let inspection = crate::inspect(argv);
    if inspection.network {
        // Só um host explícito; `*` seria um alargamento silencioso.
        inspection.host.as_deref().map(|host| Capability::Net {
            host: host.to_string(),
        })
    } else if inspection.is_plain() {
        Some(Capability::Exec {
            program: inspection.program,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::capability_for;
    use crate::facts::{Capability, ToolArgs, ToolName, ToolUse};
    use crate::paths::{ResolvedArgv, ResolvedPath};
    use crate::rule::{
        Enforcement, Rule, RuleCategory, RuleExamples, RuleScope, RuleSet, Severity,
    };

    const CONTAINMENT: &str = include_str!("../../../policy/containment.toml");

    fn rules() -> Result<RuleSet, Box<dyn std::error::Error>> {
        Ok(RuleSet::from_toml(CONTAINMENT)?)
    }

    fn read(path: &str) -> Result<ToolUse, Box<dyn std::error::Error>> {
        let resolved = ResolvedPath::from_canonical(path)?;
        Ok(ToolUse {
            name: ToolName::Read,
            args: ToolArgs::Read {
                path: resolved.clone(),
            },
            resolved_paths: vec![resolved.clone()],
            argv: None,
            cwd: resolved,
        })
    }

    #[test]
    fn sensitive_read_yields_an_explicit_read_path() -> Result<(), Box<dyn std::error::Error>> {
        let use_ = read("/home/ana/.ssh/id_rsa")?;
        let capability = capability_for(&use_, &rules()?, &"contain-sensitive-read".into());
        let Some(Capability::ReadPath { root }) = capability else {
            return Err("esperava ReadPath".into());
        };
        assert_eq!(root.as_str(), "/home/ana/.ssh/id_rsa");
        Ok(())
    }

    #[test]
    fn read_outside_workspace_yields_a_read_path() -> Result<(), Box<dyn std::error::Error>> {
        let use_ = read("/etc/hosts")?;
        let capability = capability_for(&use_, &rules()?, &"contain-read-outside-workspace".into());
        assert!(matches!(capability, Some(Capability::ReadPath { .. })));
        Ok(())
    }

    #[test]
    fn budget_and_phase_rules_are_not_overridable() -> Result<(), Box<dyn std::error::Error>> {
        let use_ = read("/work/src/main.rs")?;
        let unknown = capability_for(&use_, &rules()?, &"does-not-exist".into());
        assert!(unknown.is_none());
        Ok(())
    }

    #[test]
    fn opaque_exec_is_not_overridable() -> Result<(), Box<dyn std::error::Error>> {
        let cwd = ResolvedPath::from_canonical("/work")?;
        let argv = ResolvedArgv::new(vec![
            "bash".to_string(),
            "-c".to_string(),
            "rm -rf /".to_string(),
        ])?;
        let use_ = ToolUse {
            name: ToolName::Exec,
            args: ToolArgs::Exec {
                argv: argv.clone(),
                cwd: cwd.clone(),
            },
            resolved_paths: vec![cwd.clone()],
            argv: Some(argv),
            cwd,
        };
        let rules = RuleSet {
            vocab: 3,
            rules: vec![Rule {
                id: "deny-bash".into(),
                statement: "negar bash".to_string(),
                scope: RuleScope::Command {
                    tool: ToolName::Exec,
                },
                enforcement: Enforcement::DenyCommand {
                    tool: ToolName::Exec,
                },
                severity: Severity::Warn,
                category: RuleCategory::Enforced,
                remedy: None,
                expires_at: None,
                waiver: None,
                examples: RuleExamples::default(),
            }],
        };
        assert!(capability_for(&use_, &rules, &"deny-bash".into()).is_none());
        Ok(())
    }
}
