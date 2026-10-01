#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: relatório da confiança medida"
)]
//! `policy:confidence` (Q-11/F6) — a categoria `Enforced` **medida** a partir do log.
//!
//! Lê os `session.v{N}.jsonl` (os passados em `args`, ou todos os de `.katu/sessions`) com o leitor
//! de produção ([`read_records`], que valida `seq` contíguo e linhas ilegíveis), extrai os ensaios
//! por regra ([`rule_trials`]) e imprime o veredicto de cada regra **declarada** `Enforced`
//! (veredicto + evidência: `n`, sucessos, LB de Wilson).
//!
//! Falha quando há **contradição medida**: `n ≥ n_min` e `LB < θ` com pelo menos uma falha — isto é,
//! o log mostra uma recusa que **correu**. *Não provado* (poucas observações) é aviso, não falha: um
//! projeto novo não tem ensaios, e tratar a ausência de dados como contradição seria ruído.
//!
//! A categoria declarada **não** é reescrita: o TOML continua a ser a autoridade; o que este
//! comando publica é se o log a sustenta.

use std::fs;
use std::path::{Path, PathBuf};

use katu_core::kernel::{
    Event, LOG_SCHEMA_VERSION, enforced_verdicts, read_records, rule_trials, tool_trials,
};
use katu_core::ports::{Fs as _, MemFs};
use katu_policy::{RuleSet, Threshold, ToolName, Verdict};

use crate::walk::collect_rule_files;

/// Pasta default das sessões.
const SESSIONS_DIR: &str = ".katu/sessions";

/// Pasta default dos artefactos de política.
const POLICY_DIR: &str = "policy";

/// Mede a confiança e falha em contradição medida.
pub(crate) fn policy_confidence(args: &[String]) -> Result<(), String> {
    let files = log_files(args);
    // Sem `args` e sem sessões locais (clone limpo/CI): não há nada para medir — passar, não falhar.
    // Com `args` explícitos, um caminho ausente é erro (o chamador pediu aquele log).
    if files.is_empty() {
        if args.is_empty() {
            println!(
                "  sem sessões em {SESSIONS_DIR}/*/session.v{LOG_SCHEMA_VERSION}.jsonl: nada a medir"
            );
            return Ok(());
        }
        return Err(format!(
            "policy:confidence: nenhum log de sessão em {SESSIONS_DIR}/*/session.v{LOG_SCHEMA_VERSION}.jsonl"
        ));
    }
    let rules = load_rules()?;
    let mut events = Vec::new();
    for file in &files {
        events.extend(events_of(file)?);
    }
    let verdicts = enforced_verdicts(&events, &rules, &Threshold::DEFAULT);
    let trials = rule_trials(&events);
    let contradictions = print_rules(&verdicts);
    print_summary(&files, &events, &trials, &verdicts, &events_tools(&events));
    if contradictions.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "policy:confidence: {} contradição(ões) medida(s):\n  {}",
            contradictions.len(),
            contradictions.join("\n  ")
        ))
    }
}

/// Imprime a tabela de regras e devolve as contradições medidas.
fn print_rules(verdicts: &[Verdict]) -> Vec<String> {
    let mut contradictions: Vec<String> = Vec::new();
    println!(
        "  {:32} {:>5} {:>5} {:>6}  veredicto",
        "regra", "n", "ok", "LB"
    );
    for verdict in verdicts {
        println!(
            "  {:32} {:>5} {:>5} {:>6}  {}",
            verdict.id.as_str(),
            verdict.trials,
            verdict.successes,
            verdict.lower_milli,
            verdict.confidence.as_str()
        );
        if verdict.contradiction {
            contradictions.push(format!("{}: {}", verdict.id.as_str(), verdict.reason));
        }
    }
    contradictions
}

/// Imprime o resumo (regras, logs, eventos e as tools mais chamadas).
fn print_summary(
    files: &[PathBuf],
    events: &[Event],
    trials: &std::collections::BTreeMap<katu_policy::RuleId, katu_policy::Trials>,
    verdicts: &[Verdict],
    tools: &[(ToolName, u32, u32)],
) {
    let unmeasured = verdicts
        .iter()
        .filter(|verdict| verdict.confidence.as_str() == "unmeasured")
        .count();
    let fired = trials.values().filter(|entry| entry.trials() > 0).count();
    let rules = verdicts.len();
    let logs = files.len();
    let total = events.len();
    println!(
        "  {rules} regras Enforced, {logs} logs, {total} eventos, {fired} regras com ensaios, {unmeasured} sem observações"
    );
    for (name, calls, ok) in tools {
        println!("  tool {:24} n = {calls:>4}  ok = {ok:>4}", name.as_str());
    }
}

/// As tools mais chamadas (ordem determinística: por chamadas e depois por nome).
fn events_tools(events: &[Event]) -> Vec<(ToolName, u32, u32)> {
    let mut tools: Vec<(ToolName, u32, u32)> = tool_trials(events)
        .into_iter()
        .map(|(name, entry)| (name, entry.trials(), entry.successes()))
        .collect();
    tools.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.as_str().cmp(right.0.as_str()))
    });
    tools.truncate(5);
    tools
}

/// Eventos de um log de sessão, pelo leitor de produção (valida `seq` e linhas).
fn events_of(path: &Path) -> Result<Vec<Event>, String> {
    let bytes = fs::read(path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
    let fs = MemFs::new();
    fs.write_atomic(path, &bytes)
        .map_err(|err| format!("espelhando {}: {err}", path.display()))?;
    let records = read_records(&fs, path).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(records.into_iter().map(|record| record.event).collect())
}

/// Regras de todos os `policy/*.toml` com `[[rules]]` (a autoridade declarada).
///
/// O filtro é o mesmo do `policy:audit` ([`collect_rule_files`]): `prices.toml`/`tiers.toml` não
/// são regras e falhariam o `vocab`.
fn load_rules() -> Result<RuleSet, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    if !Path::new(POLICY_DIR).is_dir() {
        // Clone limpo sem `policy/`: nada declarado, nada a verificar.
        return Ok(RuleSet {
            vocab: katu_policy::POLICY_VOCAB_VERSION,
            rules: Vec::new(),
        });
    }
    collect_rule_files(Path::new(POLICY_DIR), &mut files)?;
    files.sort();
    let mut rules = Vec::new();
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let set = RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        rules.extend(set.rules);
    }
    Ok(RuleSet {
        vocab: katu_policy::POLICY_VOCAB_VERSION,
        rules,
    })
}

/// Logs a ler: os passados em `args`, ou todos os das sessões.
fn log_files(args: &[String]) -> Vec<PathBuf> {
    if !args.is_empty() {
        return args.iter().map(PathBuf::from).collect();
    }
    let name = format!("session.v{LOG_SCHEMA_VERSION}.jsonl");
    let Ok(sessions) = fs::read_dir(SESSIONS_DIR) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = sessions
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&name))
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::{log_files, policy_confidence};

    #[test]
    fn explicit_args_are_used_verbatim() {
        let args = vec!["a.jsonl".to_string(), "b.jsonl".to_string()];
        assert_eq!(log_files(&args).len(), 2);
    }

    #[test]
    fn a_missing_session_dir_is_not_an_error_but_an_empty_list() {
        let files = log_files(&[]);
        assert!(files.iter().all(|path| path.is_file()));
    }

    #[test]
    fn a_missing_log_is_reported() {
        // Com caminhos explícitos, um log ausente é erro (não se inventa medição).

        let error = policy_confidence(&["/nao/existe.jsonl".to_string()])
            .err()
            .unwrap_or_default();
        assert!(error.contains("lendo"), "{error}");
    }
}
