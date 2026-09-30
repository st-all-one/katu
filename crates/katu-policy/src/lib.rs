//! `katu-policy` — E02: o motor de política do katu.
//!
//! Avalia **factos tipados** e devolve um veredicto determinístico, sem I/O (DF2). O vocabulário de
//! regras é **fechado e versionado**: `RuleScope` e `Enforcement` são a superfície inteira; alargar
//! é uma decisão de kernel registada, nunca configuração de utilizador.
//!
//! ```
//! use katu_policy::{evaluate, Facts, Phase, RuleSet, ToolName, ToolUse, ToolArgs, ResolvedPath, BudgetState};
//! use std::collections::BTreeSet;
//!
//! # fn main() -> Result<(), katu_policy::PolicyError> {
//! let cwd = ResolvedPath::from_canonical("/work")?;
//! let facts = Facts {
//!     now_millis: 0,
//!     phase: Phase::Task,
//!     tool: ToolUse {
//!         name: ToolName::Read,
//!         args: ToolArgs::Read { path: cwd.clone() },
//!         resolved_paths: vec![cwd.clone()],
//!         argv: None,
//!         cwd,
//!     },
//!     capabilities: Vec::new(),
//!     budget: BudgetState::default(),
//!     completed: BTreeSet::new(),
//! };
//! let rules = RuleSet { vocab: 2, rules: Vec::new() };
//! assert!(evaluate(&facts, &rules)?.is_allow());
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "módulos internos usam pub(crate); a API pública é a reexportação em `lib.rs`"
)]

mod approval;
mod argv;
mod audit;
mod decision;
mod engine;
mod error;
mod evaluate;
mod facts;
mod glob;
mod paths;
mod rule;

pub use approval::{capability_for, capability_for_request};
pub use argv::{ArgvInspection, inspect};
pub use audit::{Activity, AuditIssue, AuditReport, ExampleCoverage, RuleSummary, audit};
pub use decision::{ApprovalRequest, ControlId, Decision, Evidence, Reason};
pub use error::PolicyError;
pub use evaluate::evaluate;
pub use facts::{
    BudgetState, Capability, Facts, Phase, SearchMode, Timestamp, ToolArgs, ToolName, ToolUse,
};
pub use glob::matches_glob;
pub use paths::{ResolvedArgv, ResolvedPath};
pub use rule::{
    BudgetCap, Enforcement, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet, Severity,
    Waiver,
};

/// Versão do vocabulário de regras (`POLICY_VOCAB_VERSION`).
///
/// O motor recusa um `RuleSet` com uma versão desconhecida (fail-closed, E02).
pub const POLICY_VOCAB_VERSION: u32 = 2;

#[cfg(test)]
mod tests {
    use super::POLICY_VOCAB_VERSION;
    use std::hint::black_box;

    #[test]
    fn vocab_version_is_tracked() {
        let version = black_box(POLICY_VOCAB_VERSION);
        assert_eq!(version, 2);
    }
}
