//! `prime` (E20-T01): contexto de arranque **estático**, byte-idêntico por versão.
//!
//! Serve a IA e o utilizador. **Não** inclui o `AGENTS.md` (esse entra no contexto do turno, por
//! ser específico do projeto — E20-T13); o prime é o contrato da superfície, não o do repositório.

use clap::{Args, ValueEnum};
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use serde::Deserialize;
use serde_json::json;

use crate::report::Report;

use super::params;

/// Grupo cujo prime se emite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Group {
    /// Visão global da superfície.
    Global,
    /// Grupo `memo`.
    Memo,
    /// Grupo `run`.
    Run,
    /// Grupo `tui`.
    Tui,
    /// Grupo `config`.
    Config,
    /// Grupo `upgrade`.
    Upgrade,
}

/// Argumentos de `katu prime`.
#[derive(Debug, Clone, Args)]
pub(crate) struct PrimeArgs {
    /// Prime longo (acrescenta gramática e escopo).
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub(crate) long: Option<bool>,
    /// Grupo a emitir (por omissão, `global`).
    #[arg(long, value_enum)]
    pub(crate) group: Option<Group>,
    /// Config universal do comando (JSON; XOR com as flags explícitas).
    #[arg(long)]
    pub(crate) params: Option<String>,
    /// Lote JSONL (uma linha = um item; XOR com `--params` e flags).
    #[arg(long)]
    pub(crate) batch: Option<String>,
    /// Emite envelope JSON em `stdout`.
    #[arg(long)]
    pub(crate) json: bool,
}

/// Parâmetros de um prime (JSON de `--params` ou de uma linha de `--batch`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimeParams {
    /// Grupo a emitir.
    group: Option<Group>,
    /// Prime longo.
    long: Option<bool>,
}

/// Executa `katu prime` (um prime ou um lote).
pub(crate) fn execute(args: &PrimeArgs) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_PRIME, "prime::execute");
    if let Some(path) = &args.batch {
        return batch(args, path);
    }
    match resolve(args) {
        Ok((group, long)) => report(group, long),
        Err(error) => Report::failed("prime", &error),
    }
}

/// Resolve grupo e `long` a partir das flags e/ou de `--params` (XOR).
fn resolve(args: &PrimeArgs) -> Result<(Group, bool), Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::CLI_PRIME, "prime::resolve");
    if args.params.is_some() && (args.long.is_some() || args.group.is_some()) {
        return Err(Error::invalid_input(
            "--params é exclusivo com flags explícitas (não se infere)",
        ));
    }
    let parsed: PrimeParams = match &args.params {
        Some(raw) => params::parse(&params::source(raw)?)?,
        None => PrimeParams::default(),
    };
    Ok((
        args.group.or(parsed.group).unwrap_or(Group::Global),
        args.long.or(parsed.long).unwrap_or(false),
    ))
}

/// Processa um lote JSONL: valida tudo **antes** de emitir.
fn batch(args: &PrimeArgs, path: &str) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_PRIME, "prime::batch");
    if args.params.is_some() || args.long.is_some() || args.group.is_some() {
        return Report::failed(
            "prime",
            &Error::invalid_input("--batch é exclusivo com --params e flags"),
        );
    }
    let lines = match params::batch_lines(path) {
        Ok(lines) => lines,
        Err(error) => return Report::failed("prime", &error),
    };
    let mut items = Vec::with_capacity(lines.len());
    for line in &lines {
        let parsed: PrimeParams = match params::parse(line) {
            Ok(parsed) => parsed,
            Err(error) => return Report::failed("prime", &error),
        };
        let group = parsed.group.unwrap_or(Group::Global);
        let long = parsed.long.unwrap_or(false);
        items.push(json!({ "group": name(group), "prime": text(group, long) }));
    }
    Report::ok("prime", Some(json!({ "items": items })))
}

/// Relatório de um prime (grupo + texto estático).
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`long` é o flag de clap do prime"
)]
pub(crate) fn report(group: Group, long: bool) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_PRIME, "prime::report");
    Report::ok(
        "prime",
        Some(json!({ "group": name(group), "prime": text(group, long) })),
    )
}

/// Fonte única dos grupos do prime: `(variante, nome estável, texto)`.
///
/// Gera `name` e `base` da **mesma** lista, para que um grupo novo não possa divergir entre o
/// envelope JSON e o texto (S-05).
macro_rules! groups {
    ($( ($variant:ident, $name:literal, $text:ident) ),* $(,)?) => {
        /// Nome estável do grupo (para o envelope).
        pub(crate) const fn name(group: Group) -> &'static str {
            match group {
                $( Group::$variant => $name, )*
            }
        }

        /// Prime curto de cada grupo.
        const fn base(group: Group) -> &'static str {
            match group {
                $( Group::$variant => $text, )*
            }
        }
    };
}

groups! {
    (Global, "global", GLOBAL),
    (Memo, "memo", MEMO),
    (Run, "run", RUN),
    (Tui, "tui", TUI),
    (Config, "config", CONFIG),
    (Upgrade, "upgrade", UPGRADE),
}

/// Texto do prime: base estática por grupo e, com `long`, a gramática/escopo comuns.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`long` é o flag de clap do prime"
)]
pub(crate) fn text(group: Group, long: bool) -> String {
    let _span = katu_core::trace_fn!("cli::prime::text");

    let mut text = base(group).to_owned();
    if long {
        text.push_str(GRAMMAR);
    }
    text
}

/// Gramática e escopo comuns (`--long`).
const GRAMMAR: &str = "\
Gramática:
- O posicional é conteúdo: `-` lê stdin; ausente + stdin não-TTY lê stdin; ausente + TTY → exit 2.
- `--json` é por comando (nunca global); stdout = dados, stderr = logs; EPIPE = sucesso.
- Exit codes: 2 invalid_input · 3 io · 4 not_found · 7 config · 10 unavailable · 70 internal.
Escopo: um facto, um lar; o katu nunca infere a intenção do utilizador (I1).
";

/// Prime global.
const GLOBAL: &str = "\
katu — agente de código com loop possuído, política e memória.

Verbos: prime, upgrade, config, memo, run, tui.
- katu                 abre a TUI (bootstrap do .katu/ no arranque)
- katu run <body>      executa uma rodada; devolve id da sessão + exit code
- katu memo <sub>      consulta a memória: ask, knowledge, doctor, sessions, drain, prime
- katu config <sub>    configuração: get, set, unset, list
- katu prime [--long]  este contexto (estático)
- katu --init          faz o bootstrap do .katu/ e sai

Observa: id da sessão (para `--resume`), exit code da rodada, envelope com `--json`.
Não faz: não infere intenção; sem conteúdo falha; sem provider recusa.
";

/// Prime do grupo `memo` (só consulta — nunca escreve).
const MEMO: &str = "\
memo — consulta e visão geral da memória (sem escrita).

- katu memo ask [query] [--limit N]   consulta pelo caminho §42 (recall)
- katu memo knowledge                 visão geral do mapa de conhecimento
- katu memo doctor [--fix]            diagnóstico do backend (fail-closed sem adaptador)
- katu memo sessions                  lista as sessões do projeto (para `--resume`)
- katu memo drain [--status|--digest] drena/reconcilia o índice
- katu memo prime [--long]            este contexto

A escrita de memória é do **agente** (tools no loop) e do `kd`; o `memo` nunca cria notas.
";

/// Prime do grupo `run`.
const RUN: &str = "\
run — executa uma rodada e sai.

- katu run <body>                     sessão nova, uma rodada, sai
- katu run --resume [id] <body>       retoma a mais recente (sem valor) ou o id indicado
Flags: --provider --model --base --max-tokens --max-steps --compact --json.
Saída humana: texto + id da sessão em destaque; envelope: `session` + `round_exit`.
";

/// Prime do grupo `tui`.
const TUI: &str = "\
tui — abre a UI de terminal sobre o loop de turnos.

- katu            (sem verbo) abre a TUI num TTY; sem TTY falha fechado
- katu tui        idem, explícito
Flags: --provider --model --base --max-tokens --max-steps --compact --resume.
";

/// Prime do grupo `config`.
const CONFIG: &str = "\
config — configuração global única e override local.

- katu config get <key>               lê a chave (projeto > global)
- katu config set <key> <value>       escreve a chave
- katu config unset <key>             remove a chave
- katu config list [--global]         lista as chaves
Global: ~/.config/local/katu/katu.toml · Projeto: <projeto>/.katu/katu.toml (snapshot 1:1).
";

/// Prime do grupo `upgrade`.
const UPGRADE: &str = "\
upgrade — sincronização de versão.

Estado: canal ainda não configurado; recusa explicitamente (fail-closed), sem inventar origem.
Sincronização futura contra GitHub Releases (E20-T02).
";
