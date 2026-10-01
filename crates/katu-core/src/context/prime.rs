//! Prime do turno (DF12/E09-T01): a gramática do TOON colunar, o registo de esquema e — quando
//! ligada (Q-04) — a secção de estado.
//!
//! O prime é **determinístico e versionado**: o mesmo modo dá o mesmo texto, e mudar o texto exige
//! incrementar [`PRIME_VERSION`]. A secção de estado é acrescentada **no fim**, pelo que o resto do
//! prompt continua a ser um prefixo estável.

use crate::toon::schema::{self, Mode};

use super::AssembleOptions;

/// Versão do prime (DF12). Mudar o texto do prime exige incrementar isto.
pub const PRIME_VERSION: u32 = 3;

/// Variante do prime (E09-T01): compacto (default) ou completo (`--long`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimeMode {
    /// Prime compacto (default).
    #[default]
    Compact,
    /// Prime completo (spec TOON) — `--long`.
    Long,
}

/// Gramática do formato, partilhada pelo prime compacto e pelo prime com catálogo.
const PRIME_GRAMMAR: &str = "saida: TOON colunar v3 (D39) SEM headers; o esquema vive aqui:\n\
     - tabela: `\\x1eNOME` e depois linhas com celulas separadas por `\\x1f`, na ordem indicada;\n\
     - literal: `\\x1dNOME` e depois linhas cruas ate a proxima secao;\n\
     - vazio = celula ausente; `{a,b}` = dominio fechado; booleano 0/1; sem floats.\n";

/// Prime compacto (DF12): ensina a gramática do TOON colunar v3 **e o registo de esquema**.
#[must_use]
pub fn prime() -> String {
    let _span = crate::trace_fn!("context::prime");

    format!(
        "katu prime v{PRIME_VERSION}\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         {PRIME_GRAMMAR}{}\n\
         JSON com format=json; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Prime compacto com o **catálogo de tools** injetado (fonte: `katu_tools::schema::catalog`).
///
/// Substitui a linha `tools:` pela tabela `tool` do registo — o esquema e as tools partilham a
/// mesma projeção colunar, sem a lista escrita à mão.
#[must_use]
pub fn prime_with_catalog(catalog: &str) -> String {
    let _span = crate::trace_fn!("context::prime_with_catalog");

    format!(
        "katu prime v{PRIME_VERSION}\n\
         tools (tabela `tool`; `?` opcional, `{{a,b}}` dominio fechado):\n\
         {catalog}\
         {PRIME_GRAMMAR}{}\n\
         JSON com format=json; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Prime completo (spec TOON colunar v3) — `--long` (E09-T01). Determinístico e versionado.
#[must_use]
pub fn prime_long() -> String {
    let _span = crate::trace_fn!("context::prime_long");

    format!(
        "katu prime v{PRIME_VERSION} (long)\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         saida: TOON colunar v3 (D39, ADR 0005). Sem headers no stream; o esquema segue.\n\
         - tabela: linha `\\x1eNOME`; linhas seguintes com celulas `\\x1f` na ordem das colunas;\n\
         - literal: linha `\\x1dNOME`; linhas cruas ate a proxima secao;\n\
         - envelope `r` = kind,id,hash,cur,tot,trunc,bytes,ms,tok; escadares da tool em `k` (k,v);\n\
         - ids/paths podem vir como `#N`/`@N` (aliases de sessao), mapeados na secao `sym`;\n\
         - vazio = celula ausente; dominio `{{a,b}}` fechado; booleano 0/1; sem floats.\n\
         {}\n\
         JSON equivalente com `format=json`/`--json`; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Uma linha por secção do registo: `nome R col...` ou `nome L`.
fn registry_text() -> String {
    let _span = crate::trace_fn!("context::registry_text");

    let mut out = String::from("esquema:\n");
    for spec in schema::registry() {
        out.push_str(spec.name);
        match spec.mode {
            Mode::Literal => {
                out.push_str(" L\n");
            }
            Mode::Rows => {
                out.push_str(" R");
                for col in spec.cols {
                    out.push(' ');
                    out.push_str(col.name);
                    if !col.domain.is_empty() {
                        out.push('{');
                        out.push_str(&col.domain.join(","));
                        out.push('}');
                    }
                }
                out.push('\n');
            }
        }
    }
    out
}

/// Prime no modo pedido.
#[must_use]
pub fn prime_for(mode: PrimeMode) -> String {
    let _span = crate::trace_fn!("context::prime_for");

    match mode {
        PrimeMode::Compact => prime(),
        PrimeMode::Long => prime_long(),
    }
}

/// Prime do turno: o texto pedido mais a secção de estado (Q-04), se houver.
pub(super) fn prime_text(options: AssembleOptions<'_>) -> String {
    let _span = crate::trace_fn!("context::prime_text");

    let mut prime = prime_for(options.prime);
    if let Some(state) = options.state {
        prime.push_str(state);
    }
    prime
}
