//! Envelope de saída das tools (DF12/E06-T12): **contrato agnóstico**, TOON ao modelo, JSON à
//! máquina.
//!
//! Um [`ToolReport`] carrega o tipo (`kind`), ids estáveis, o payload (`data`, uma árvore
//! [`Value`]), paginação, próximas ações e custo. O **mesmo** relatório renderiza em TOON
//! ([`ToolReport::to_toon`]) e em JSON ([`ToolReport::to_json`]).

use serde::Serialize;

use crate::toon::{self, Value};

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

    fn to_value(self) -> Value {
        let mut entries = Vec::new();
        if let Some(cursor) = self.cursor {
            entries.push(("cursor".to_string(), Value::int(to_i64(cursor))));
        }
        entries.push(("total".to_string(), Value::int(to_i64(self.total))));
        entries.push(("truncated".to_string(), Value::bool(self.truncated)));
        Value::flow(entries)
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

impl Cost {
    fn to_value(self) -> Value {
        Value::flow(vec![
            ("bytes".to_string(), Value::int(to_i64(self.bytes))),
            ("ms".to_string(), Value::int(to_i64(self.ms))),
            (
                "tokens_est".to_string(),
                Value::int(to_i64(self.tokens_est)),
            ),
        ])
    }
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
        self.id = Some(id.into());
        self
    }

    /// Define o hash.
    #[must_use]
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    /// Define a página.
    #[must_use]
    pub fn with_page(mut self, page: Page) -> Self {
        self.page = Some(page);
        self
    }

    /// Define as próximas ações.
    #[must_use]
    pub fn with_next(mut self, next: Vec<String>) -> Self {
        self.next = next;
        self
    }

    /// Define o custo.
    #[must_use]
    pub fn with_cost(mut self, cost: Cost) -> Self {
        self.cost = Some(cost);
        self
    }

    /// Renderiza em **TOON** (formato ao modelo).
    #[must_use]
    pub fn to_toon(&self) -> String {
        let mut entries = vec![("kind".to_string(), Value::str(self.kind))];
        if let Some(id) = &self.id {
            entries.push(("id".to_string(), Value::str(id.clone())));
        }
        if let Some(hash) = &self.hash {
            entries.push(("hash".to_string(), Value::str(hash.clone())));
        }
        entries.push(("data".to_string(), self.data.clone()));
        if let Some(page) = self.page {
            entries.push(("page".to_string(), page.to_value()));
        }
        if !self.next.is_empty() {
            entries.push((
                "next".to_string(),
                Value::list(self.next.iter().map(|s| Value::str(s.clone())).collect()),
            ));
        }
        if let Some(cost) = self.cost {
            entries.push(("cost".to_string(), cost.to_value()));
        }
        toon::emit(&Value::map(entries))
    }

    /// Renderiza em **JSON** (alternativa de máquina).
    ///
    /// # Errors
    /// [`serde_json::Error`] se a serialização falhar (não deve ocorrer para um `ToolReport`).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Converte `u64` em `i64` para o TOON (satura no máximo).
fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Hash FNV-1a de 64 bits (determinístico, sem dependências).
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> u64 {
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
    format!("{prefix}_{:016x}", fingerprint(seed))
}

/// Hash de conteúdo em 16 hex.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:016x}", fingerprint(bytes))
}

#[cfg(test)]
mod tests;
