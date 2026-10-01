#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: relatório do portão agregado"
)]
//! `check` (E01-T10): **um único ponto de entrada** para os gates, local e em CI.
//!
//! Corre o mesmo conjunto que o `make check`: `fmt` + `clippy` + testes + limite de linhas +
//! camadas + cobertura de crates + diag + schemas + docs + memória + política + gates de número
//! (`gate:bench`/`gate:provider`/`gate:render`). Falha no primeiro gate vermelho, com a mensagem
//! agregada — o CI e o programador veem o mesmo contrato.

use std::process::Command;

use crate::bench::gate_bench;
use crate::catalog::check_catalog;
use crate::check_policy::check_policy;
use crate::coverage::check_rule_coverage;
use crate::ledger::ledger_validate;
use crate::memory_swap::check_memory_swap;
use crate::policy::policy_audit;
use crate::schemas::check_schemas;
use crate::slices::check_slices;
use crate::surface::check_surface;
use crate::unsafe_check::check_unsafe;
use crate::{
    check_crate_coverage, check_diag, check_diag_coverage, check_docs, check_layers,
    provider_bench, render_bench,
};

/// Corre um comando externo; erro com o código de saída se falhar.
fn run(label: &str, program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("{label}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} falhou ({status})"))
    }
}

/// Ponto de entrada de `check`: corre todos os gates na ordem do `make check`.
pub(crate) fn run_all() -> Result<(), String> {
    run("fmt", "cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        "clippy",
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run("test", "cargo", &["test", "--workspace"])?;
    run("file-length", "./scripts/check_file_length.sh", &[])?;
    check_layers()?;
    check_crate_coverage()?;
    check_diag()?;
    check_diag_coverage()?;
    check_schemas()?;
    check_docs()?;
    check_surface(&[])?;
    check_policy()?;
    check_unsafe()?;
    check_catalog()?;
    check_rule_coverage()?;
    check_slices()?;
    check_memory_swap()?;
    policy_audit(&[])?;
    ledger_validate(&[])?;
    gate_bench(&[])?;
    provider_bench::gate(&[])?;
    render_bench::gate(&[])?;
    println!("check ok: todos os gates verdes");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn missing_program_is_an_error() {
        assert!(run("x", "katu-no-such-binary-xyz", &[]).is_err());
    }
}
