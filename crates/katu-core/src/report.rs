//! Envelope de saída das tools (DF12/E06-T12): **contrato agnóstico**, TOON ao modelo, JSON à
//! máquina.
//!
//! Um [`ToolReport`] carrega o tipo (`kind`), ids estáveis, o payload (`data`, uma árvore
//! [`Value`]), paginação, próximas ações e custo. O **mesmo** relatório renderiza em TOON
//! ([`ToolReport::to_toon`]) e em JSON ([`ToolReport::to_json`]).

use serde::Serialize;

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::toon::{self, Aliases, Cell, RowTable, Section, Value};

/// Paginação de um resultado (cursor opaco; `total` sempre presente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Page {
    /// Cursor para a página seguinte (`None` = fim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    /// Total de itens (não só os mostrados).
    pub total: u64,
    /// Se o resultado foi truncado.
    pub truncated: bool,
}

impl Page {
    /// Página completa (sem cursor, não truncada).
    #[must_use]
    pub const fn complete(total: u64) -> Self {
        Self {
            cursor: None,
            total,
            truncated: false,
        }
    }
}

/// Custo estimado de uma resposta (**advisory**; não é métrica publicada, DF5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Cost {
    /// Bytes da resposta renderizada.
    pub bytes: u64,
    /// Tempo de parede (ms) medido na borda.
    pub ms: u64,
    /// Estimativa de tokens (base `inferred`; o modelo usa como heurística).
    pub tokens_est: u64,
}

/// Teto do **delta** que entra no log e no prompt (bytes).
///
/// O delta é o payload da tool renderizado em TOON — o que o modelo precisa para agir (§18/G6: "só
/// o delta chega ao modelo"). Acima do teto corta-se em fronteira de linha e deixa-se um
/// **ponteiro** (`kind`/`id`) para o modelo pedir uma página (`read` com `range`/`view`): o
/// envelope já traz `trunc`/`cur`/`tot` quando a tool pagina, e este teto é a rede de segurança
/// para as que não paginam.
pub const MAX_DELTA_BYTES: usize = 8_192;

/// Conteúdo que o modelo recebe por um resultado de tool: o **delta** (payload TOON, §18/G6) quando
/// existe; caso contrário a serialização do efeito (recusa com evidência/remédio, indisponível).
///
/// É o ponto único dos dialetos: todos os providers entregam a mesma string ao modelo.
#[must_use]
pub fn tool_content(outcome: &ToolOutcome, delta: Option<&str>) -> String {
    let _span = crate::trace_fn!("report::tool_content");

    if let Some(delta) = delta {
        return delta.to_string();
    }
    serde_json::to_string(outcome).unwrap_or_else(|_| String::from("{}"))
}

/// Envelope tipado da saída de uma tool.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolReport {
    /// Identificador estável do tipo de relatório (ex.: `read.summary`).
    pub kind: &'static str,
    /// Id content-addressed da entidade principal (opcional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Hash do conteúdo (opcional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Payload (árvore TOON).
    pub data: Value,
    /// Paginação (opcional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<Page>,
    /// Próximas ações sugeridas (determinísticas).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    /// Custo estimado (opcional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

impl ToolReport {
    /// Cria um relatório com o tipo e o payload.
    #[must_use]
    pub const fn new(kind: &'static str, data: Value) -> Self {
        Self {
            kind,
            id: None,
            hash: None,
            data,
            page: None,
            next: Vec::new(),
            cost: None,
        }
    }

    /// Define o id.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        let _span = crate::trace_fn!("report::with_id");

        self.id = Some(id.into());
        self
    }

    /// Define o hash.
    #[must_use]
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        let _span = crate::trace_fn!("report::with_hash");

        self.hash = Some(hash.into());
        self
    }

    /// Define a página.
    #[must_use]
    pub fn with_page(mut self, page: Page) -> Self {
        let _span = crate::trace_fn!("report::with_page");

        self.page = Some(page);
        self
    }

    /// Define as próximas ações.
    #[must_use]
    pub fn with_next(mut self, next: Vec<String>) -> Self {
        let _span = crate::trace_fn!("report::with_next");

        self.next = next;
        self
    }

    /// Define o custo.
    #[must_use]
    pub fn with_cost(mut self, cost: Cost) -> Self {
        let _span = crate::trace_fn!("report::with_cost");

        self.cost = Some(cost);
        self
    }

    /// Renderiza em **TOON colunar v3** (formato ao modelo, ADR 0005), sem aliases.
    #[must_use]
    pub fn to_toon(&self) -> String {
        let _span = crate::fn_span!(Level::Debug, events::TOON_EMIT, "report::to_toon", "kind" => self.kind);
        let mut sections = vec![self.envelope(self.id.as_deref().unwrap_or_default())];
        sections.extend(toon::project(&self.data));
        self.push_next(&mut sections);
        toon::emit(&sections)
    }

    /// Delta **model-visible**: o TOON do envelope, cortado ao teto [`MAX_DELTA_BYTES`].
    ///
    /// É este texto que entra no log (evento `ToolResult.delta`) e no pedido ao provider — a
    /// mesma string, para que `Model-visible ⟺ logged` seja exato (E04).
    #[must_use]
    pub fn to_delta(&self) -> String {
        let _span = crate::fn_span!(Level::Debug, events::TOON_EMIT, "report::to_delta", "kind" => self.kind);

        let text = self.to_toon();
        if text.len() <= MAX_DELTA_BYTES {
            return text;
        }
        let cut = text
            .char_indices()
            .map(|(index, ch)| index.saturating_add(ch.len_utf8()))
            .take_while(|end| *end <= MAX_DELTA_BYTES)
            .last()
            .unwrap_or(0);
        let prefix = text.get(..cut).unwrap_or("");
        let prefix = prefix
            .rfind('\n')
            .map_or(prefix, |end| prefix.get(..end).unwrap_or(prefix));
        format!(
            "{prefix}\n# delta truncado em {MAX_DELTA_BYTES} B; kind={}; id={}\n",
            self.kind,
            self.id.as_deref().unwrap_or("-")
        )
    }

    /// Renderiza com **aliases de sessão** (`#N`/`@N`) e a secção `sym` dos novos.
    #[must_use]
    pub fn to_toon_with(&self, aliases: &mut Aliases) -> String {
        let _span = crate::fn_span!(
            Level::Debug,
            events::TOON_EMIT,
            "report::to_toon_with",
            "kind" => self.kind
        );
        let (data, mut fresh) = aliases.substitute(&self.data);
        let id = self.id.as_deref().map_or_else(String::new, |raw| {
            let (alias, pair) = aliases.intern_id(raw);
            if let Some(pair) = pair {
                fresh.insert(0, pair);
            }
            alias
        });
        let mut sections = Vec::new();
        if !fresh.is_empty() {
            let mut table = RowTable::new("sym");
            for (alias, value) in &fresh {
                table.push(vec![Cell::text(alias.clone()), Cell::text(value.clone())]);
            }
            sections.push(Section::Rows(table));
        }
        sections.push(self.envelope(&id));
        sections.extend(toon::project(&data));
        self.push_next(&mut sections);
        toon::emit(&sections)
    }

    fn push_next(&self, sections: &mut Vec<Section<'_>>) {
        let _span = crate::trace_fn!("report::push_next");

        if self.next.is_empty() {
            return;
        }
        let mut table = RowTable::new("next");
        for action in &self.next {
            table.push(vec![Cell::text(action.clone())]);
        }
        sections.push(Section::Rows(table));
    }

    /// Envelope universal `r` (colunas fixas; ausente = célula vazia).
    fn envelope<'a>(&'a self, id: &'a str) -> Section<'a> {
        let _span = crate::trace_fn!("report::envelope");

        let (cursor, total, truncated) = self.page.map_or((None, 0, false), |page| {
            (page.cursor.map(to_i64), to_i64(page.total), page.truncated)
        });
        let (bytes, ms, tokens) = self.cost.map_or((0, 0, 0), |cost| {
            (to_i64(cost.bytes), to_i64(cost.ms), to_i64(cost.tokens_est))
        });
        let row = vec![
            Cell::text(self.kind),
            Cell::text(id),
            Cell::optional(self.hash.as_deref()),
            cursor.map_or_else(|| Cell::text(""), Cell::int),
            Cell::int(total),
            Cell::bool(truncated),
            Cell::int(bytes),
            Cell::int(ms),
            Cell::int(tokens),
        ];
        let mut table = RowTable::new("r");
        table.push(row);
        Section::Rows(table)
    }

    /// Renderiza em **JSON** (alternativa de máquina).
    ///
    /// # Errors
    /// [`serde_json::Error`] se a serialização falhar (não deve ocorrer para um `ToolReport`).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let _span = crate::trace_fn!("report::to_json");

        serde_json::to_string(self)
    }
}

/// Converte `u64` em `i64` para o TOON (satura no máximo).
fn to_i64(value: u64) -> i64 {
    let _span = crate::trace_fn!("report::to_i64");

    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Hash FNV-1a de 64 bits (determinístico, sem dependências).
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let _span = crate::trace_fn!("report::fingerprint");

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Id content-addressed com prefixo (`f_`/`s_`/…), 16 hex estáveis (64 bits — sem colisões a escala).
#[must_use]
pub fn content_id(prefix: &str, seed: &[u8]) -> String {
    let _span = crate::trace_fn!("report::content_id");

    format!("{prefix}_{:016x}", fingerprint(seed))
}

/// Hash de conteúdo em 16 hex.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let _span = crate::trace_fn!("report::content_hash");

    format!("{:016x}", fingerprint(bytes))
}

#[cfg(test)]
mod tests;
