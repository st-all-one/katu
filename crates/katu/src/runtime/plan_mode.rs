//! Modo de planeamento (E20-T11): regra Enforced “escrita só sob `.katu/`” + artefacto de plano.
//!
//! Vive num módulo filho para manter `runtime.rs` sob o teto de linhas. O modo é **estado do
//! runtime** (não persistido): ao ligar, injeta a regra `plan-write-only-katu` no `RuleSet` e
//! registra-a no log; ao desligar, remove-a. A regra é `DenyWriteOutside`: nenhuma capacidade a
//! destranca (ADR 0022). O artefacto `.md` é escrito pela borda via a porta [`Fs`].

use std::path::PathBuf;

use katu_core::diag::{Level, events};
use katu_core::ports::{Fs, FsError};
use katu_policy::{
    Enforcement, PolicyError, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId, RuleScope,
    Severity, ToolName,
};

use super::Runtime;

/// Id estável da regra do modo plano.
pub(crate) const PLAN_RULE_ID: &str = "plan-write-only-katu";

/// Id estável da regra que bloqueia shell no modo plano.
pub(crate) const PLAN_SHELL_RULE_ID: &str = "plan-no-shell";

/// Diretório dos artefactos de plano, relativo à raiz do projeto.
pub(crate) const PLAN_DIR: &str = ".katu/plan";

impl Runtime<'_> {
    /// `true` se o modo de planeamento está ligado (E20-T11).
    #[must_use]
    pub(crate) const fn plan_mode(&self) -> bool {
        self.plan_mode
    }

    /// Liga/desliga o modo de planeamento e devolve o estado final (E20-T11).
    ///
    /// # Errors
    /// [`PolicyError`] se a raiz `.katu` ou o âmbito `/` não forem resolvíveis.
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "toggle explícito (`on`), mais legível que um enum de dois valores"
    )]
    pub(crate) fn set_plan_mode(&mut self, on: bool) -> Result<bool, PolicyError> {
        if on == self.plan_mode {
            return Ok(self.plan_mode);
        }
        if on {
            let allowed = ResolvedPath::from_canonical(self.root().join(".katu"))?;
            let anywhere = ResolvedPath::from_canonical("/")?;
            self.rules.rules.push(plan_rule(allowed, anywhere));
            self.rules.rules.push(shell_rule());
            self.plan_mode = true;
            katu_core::event!(
                Level::Info,
                events::PLAN_MODE,
                "on" => true,
                "rule" => PLAN_RULE_ID
            );
        } else {
            self.rules
                .rules
                .retain(|rule| !is_plan_rule(rule.id.as_str()));
            self.plan_mode = false;
            katu_core::event!(Level::Info, events::PLAN_MODE, "on" => false);
        }
        Ok(self.plan_mode)
    }

    /// Escreve o esqueleto do plano em `.katu/plan/<yymmddhhmmZ>-<slug>.md` (E20-T11).
    ///
    /// # Errors
    /// [`FsError`] se o diretório ou a escrita atómica falharem.
    pub(crate) fn write_plan_artifact(&self, fs: &dyn Fs) -> Result<PathBuf, FsError> {
        let dir = self.root().join(PLAN_DIR);
        fs.create_dir_all(&dir)?;
        let stamp = utc_stamp(self.clock.now().as_millis());
        let path = dir.join(format!("{stamp}-{}.md", slugify(&self.goal)));
        fs.write_atomic(&path, plan_markdown(&self.goal).as_bytes())?;
        Ok(path)
    }
}

/// `true` se o id pertence às regras injetadas pelo modo plano.
fn is_plan_rule(id: &str) -> bool {
    id == PLAN_RULE_ID || id == PLAN_SHELL_RULE_ID
}

/// Regra crítica que nega shell no modo plano (nenhuma capacidade destranca).
fn shell_rule() -> Rule {
    Rule {
        id: RuleId::from(PLAN_SHELL_RULE_ID),
        statement: "Modo de planeamento: sem shell (`!`)".to_string(),
        scope: RuleScope::Command {
            tool: ToolName::Exec,
        },
        enforcement: Enforcement::DenyCommand {
            tool: ToolName::Exec,
        },
        severity: Severity::Critical,
        category: RuleCategory::Enforced,
        expires_at: None,
        waiver: None,
        examples: RuleExamples {
            negative: vec!["!rm -rf no modo plano".to_string()],
            positive: Vec::new(),
        },
    }
}

/// Regra crítica que nega escrita fora de `allowed` (nenhuma capacidade destranca).
fn plan_rule(allowed: ResolvedPath, anywhere: ResolvedPath) -> Rule {
    Rule {
        id: RuleId::from(PLAN_RULE_ID),
        statement: "Modo de planeamento: escrita só sob `.katu/`".to_string(),
        scope: RuleScope::Path { root: anywhere },
        enforcement: Enforcement::DenyWriteOutside { root: allowed },
        severity: Severity::Critical,
        category: RuleCategory::Enforced,
        expires_at: None,
        waiver: None,
        examples: RuleExamples {
            negative: vec!["write src/main.rs no modo plano".to_string()],
            positive: vec!["write .katu/plan/x.md no modo plano".to_string()],
        },
    }
}

/// Esqueleto denso do plano (o modelo preenche-o em `.katu/`).
fn plan_markdown(goal: &str) -> String {
    format!(
        "# Plano — {goal}\n\n\
         > Gerado pelo modo de planeamento (`/plan`). Escrita só sob `.katu/`.\n\n\
         ## Contexto\n\n{goal}\n\n\
         ## Passos\n\n1. \n\n\
         ## Critérios de aceite\n\n- \n\n\
         ## Rollback\n\n- \n"
    )
}

/// Marca UTC compacta `yymmddhhmmZ` a partir de milissegundos desde a época.
#[must_use]
pub(crate) fn utc_stamp(millis: u64) -> String {
    let seconds = millis / 1_000;
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rem = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let short = year.rem_euclid(100);
    format!("{short:02}{month:02}{day:02}{hour:02}{minute:02}Z")
}

/// Data civil (ano, mês, dia) a partir de dias desde a época (algoritmo de Hinnant).
#[allow(
    clippy::arithmetic_side_effects,
    reason = "conversão civil de Hinnant sobre dias não-negativos (sem risco de overflow)"
)]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Slug determinístico do objetivo para o nome do ficheiro.
#[must_use]
pub(crate) fn slugify(goal: &str) -> String {
    let mut slug = String::new();
    for character in goal.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    let capped: String = slug.chars().take(40).collect();
    if capped.is_empty() {
        "plano".to_string()
    } else {
        capped
    }
}

#[cfg(test)]
mod tests {
    use super::{slugify, utc_stamp};

    #[test]
    fn utc_stamp_uses_the_compact_format() {
        // 2026-09-30T14:29:00Z
        assert_eq!(utc_stamp(1_790_778_540_000), "2609301429Z");
        assert_eq!(utc_stamp(0), "7001010000Z");
    }

    #[test]
    fn slugify_is_deterministic_and_capped() {
        assert_eq!(slugify("Corrige o bug do @plan!"), "corrige-o-bug-do-plan");
        assert_eq!(slugify("!!!"), "plano");
        assert!(slugify(&"a".repeat(100)).len() <= 40);
    }
}
