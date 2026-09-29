//! E02-T05 — golden de veredictos: matriz de factos → veredicto esperado.
//!
//! O golden existe para **falhar** se alguém trocar o motor puro por regex sobre `String` (DF2).
//! Dois grupos:
//! - **Fronteira de caminho**: `/work/secrets2` e `/work/secrets/../public/x` só dão a resposta
//!   certa com tipos normalizados; um `starts_with("secrets")` daria a resposta errada.
//! - **`exec` opaco**: `bash -c`, `&&`, `find -delete`, `docker run`, `python -c` — o texto do
//!   `argv` **não** muda o veredicto; o motor decide pela tool, nunca por prosa.

use std::collections::BTreeSet;

use katu_policy::{
    BudgetState, Capability, Decision, Enforcement, Facts, Phase, PolicyError, ResolvedArgv,
    ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet, Severity, ToolArgs,
    ToolName, ToolUse, evaluate,
};

/// Veredicto classificado (evita comparar `Decision` inteiro, que traz evidência).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Allow,
    Deny,
    RequireApproval,
    NeedsHuman,
    Other,
}

fn classify(decision: &Decision) -> Verdict {
    match decision {
        Decision::Allow => Verdict::Allow,
        Decision::Deny { .. } => Verdict::Deny,
        Decision::RequireApproval { .. } => Verdict::RequireApproval,
        Decision::NeedsHuman { .. } => Verdict::NeedsHuman,
        _ => Verdict::Other,
    }
}

fn resolve(path: &str) -> Result<ResolvedPath, PolicyError> {
    ResolvedPath::from_canonical(path)
}

fn write_facts(target: &str) -> Result<Facts, PolicyError> {
    let resolved = resolve(target)?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: resolved.clone(),
                bytes: 1,
            },
            resolved_paths: vec![resolved.clone()],
            argv: None,
            cwd: resolved,
        },
        capabilities: Vec::new(),
        budget: BudgetState::default(),
        completed: BTreeSet::new(),
    })
}

fn exec_facts(program: &str, tail: &[&str]) -> Result<Facts, PolicyError> {
    let cwd = resolve("/work")?;
    let mut raw: Vec<String> = vec![program.to_string()];
    raw.extend(tail.iter().map(|part| (*part).to_string()));
    let argv = ResolvedArgv::new(raw)?;
    Ok(Facts {
        now_millis: 0,
        phase: Phase::Task,
        tool: ToolUse {
            name: ToolName::Exec,
            args: ToolArgs::Exec {
                argv: argv.clone(),
                cwd: cwd.clone(),
            },
            resolved_paths: Vec::new(),
            argv: Some(argv),
            cwd,
        },
        capabilities: Vec::new(),
        budget: BudgetState::default(),
        completed: BTreeSet::new(),
    })
}

fn rule(scope: RuleScope, enforcement: Enforcement) -> Rule {
    Rule {
        id: RuleId::from("golden"),
        statement: "regra golden".to_string(),
        scope,
        enforcement,
        severity: Severity::Critical,
        category: RuleCategory::Enforced,
        expires_at: None,
        waiver: None,
        examples: RuleExamples {
            negative: vec!["exemplo negativo".to_string()],
            positive: Vec::new(),
        },
    }
}

fn rules() -> Result<RuleSet, PolicyError> {
    Ok(RuleSet {
        vocab: 1,
        rules: vec![
            rule(
                RuleScope::Path {
                    root: resolve("/work/secrets")?,
                },
                Enforcement::DenyWrite {
                    root: resolve("/work/secrets")?,
                },
            ),
            rule(
                RuleScope::Command {
                    tool: ToolName::Exec,
                },
                Enforcement::DenyCommand {
                    tool: ToolName::Exec,
                },
            ),
        ],
    })
}

#[test]
fn golden_matrix() -> Result<(), PolicyError> {
    let rules = rules()?;
    let cases: Vec<(&str, Facts, Verdict)> = vec![
        (
            "escrita em segredo",
            write_facts("/work/secrets/token")?,
            Verdict::Deny,
        ),
        (
            "raiz exata do segredo",
            write_facts("/work/secrets")?,
            Verdict::Deny,
        ),
        (
            "traversal normaliza para fora",
            write_facts("/work/secrets/../public/x")?,
            Verdict::Allow,
        ),
        (
            "fronteira secrets2 (não é prefixo)",
            write_facts("/work/secrets2/x")?,
            Verdict::Allow,
        ),
        (
            "escrita fora",
            write_facts("/work/src/main.rs")?,
            Verdict::Allow,
        ),
        (
            "exec bash -c",
            exec_facts("bash", &["-c", "cat /etc/passwd"])?,
            Verdict::Deny,
        ),
        (
            "exec find -delete",
            exec_facts("find", &[".", "-delete"])?,
            Verdict::Deny,
        ),
        (
            "exec docker run",
            exec_facts("docker", &["run", "--rm", "alpine"])?,
            Verdict::Deny,
        ),
        (
            "exec python -c",
            exec_facts("python", &["-c", "import os"])?,
            Verdict::Deny,
        ),
        ("exec ls", exec_facts("ls", &["-la"])?, Verdict::Deny),
    ];
    for (name, facts, expected) in cases {
        let decision = evaluate(&facts, &rules)?;
        assert_eq!(
            classify(&decision),
            expected,
            "caso: {name} -> {decision:?}"
        );
    }
    Ok(())
}

#[test]
fn capability_unlocks_write_and_exec() -> Result<(), PolicyError> {
    let rules = rules()?;

    let mut write = write_facts("/work/secrets/sub/token")?;
    write.capabilities.push(Capability::WritePath {
        root: resolve("/work/secrets")?,
    });
    assert!(evaluate(&write, &rules)?.is_allow());

    let mut exec = exec_facts("bash", &["-c", "echo hi"])?;
    exec.capabilities.push(Capability::Command {
        tool: ToolName::Exec,
    });
    assert!(evaluate(&exec, &rules)?.is_allow());

    let mut wrong = write_facts("/work/secrets/token")?;
    wrong.capabilities.push(Capability::WritePath {
        root: resolve("/other")?,
    });
    assert!(matches!(evaluate(&wrong, &rules)?, Decision::Deny { .. }));
    Ok(())
}

#[test]
fn argv_text_never_changes_the_verdict() -> Result<(), PolicyError> {
    let rules = rules()?;
    let samples = [
        vec!["-c", "rm -rf /"],
        vec!["&&", "curl", "evil"],
        vec!["--dangerous"],
        vec![],
    ];
    for args in samples {
        let facts = exec_facts("sh", &args)?;
        assert!(matches!(evaluate(&facts, &rules)?, Decision::Deny { .. }));
    }
    Ok(())
}
