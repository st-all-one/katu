//! Porta `Provider` (E12): **endpoint de modelo**, streaming normalizado.
//!
//! O provider é o **cliente do plano de dados** (DF8): fala o protocolo de um endpoint de modelo e
//! devolve deltas normalizados. Não possui o loop, a sessão nem a política — isso é do kernel
//! (DF1). Implementações: `katu-providers` (built-in `opencode go/zen`, `llama.cpp` local e o
//! fake determinístico dos testes).
//!
//! Este módulo define **só o contrato**: nada aqui fala com a rede, e o núcleo compila com ou sem
//! qualquer provider ligado (firewall LLM-free, E12-T01).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Error;
use crate::evidence::EvidenceBasis;
use crate::kernel::{CallId, Message};

/// Grau de pensamento (reasoning) pedido ao modelo num turno (E12-T10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Thinking {
    /// Sem pensamento explícito (o mínimo que o modelo permitir).
    #[default]
    Off,
    /// Pouco.
    Low,
    /// Médio.
    Medium,
    /// Muito.
    High,
}

/// Modelo + grau de pensamento de um turno.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Identificador do modelo no endpoint.
    pub model: String,
    /// Grau de pensamento.
    pub thinking: Thinking,
}

impl ModelSpec {
    /// Constrói a partir do id, com pensamento desligado.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            thinking: Thinking::Off,
        }
    }
}

/// Definição de tool enviada ao modelo (catálogo do katu).
///
/// `name` é o **nome ao modelo** (`bash`, `grep`, `find`, `ls`, `memory`, …), que não coincide
/// necessariamente com o `ToolName` de política (`exec`, `search`, …): a tradução é do registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    /// Nome ao modelo (superfície fechada do registry).
    pub name: String,
    /// Descrição de uma linha.
    pub description: String,
    /// JSON Schema dos parâmetros.
    pub parameters: Value,
}

/// Pedido normalizado a um endpoint de modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    /// Modelo + pensamento.
    pub model: ModelSpec,
    /// Instrução de sistema / prime (opcional).
    pub system: Option<String>,
    /// Histórico visível ao modelo (projeção do log).
    pub messages: Vec<Message>,
    /// Catálogo de tools disponíveis.
    pub tools: Vec<ToolDef>,
    /// Teto de tokens de saída (opcional).
    pub max_tokens: Option<u32>,
    /// Temperatura de amostragem (opcional).
    pub temperature: Option<f32>,
}

/// Contabilização de tokens de uma chamada, com a sua base (DF5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens de entrada.
    pub input: Option<u64>,
    /// Tokens de saída.
    pub output: Option<u64>,
    /// Tokens de entrada servidos por prefix-cache (E18/F4), quando reportado.
    pub cached_input: Option<u64>,
    /// Tokens de raciocínio (thinking), quando reportado.
    pub reasoning: Option<u64>,
    /// Base de evidência dos números.
    pub basis: EvidenceBasis,
}

impl TokenUsage {
    /// Nova contabilização com a base indicada.
    #[must_use]
    pub const fn new(basis: EvidenceBasis) -> Self {
        Self {
            input: None,
            output: None,
            cached_input: None,
            reasoning: None,
            basis,
        }
    }
}

/// Por que razão o modelo parou de gerar.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// Fim natural do turno.
    EndTurn,
    /// O modelo pediu tools (o loop deve executá-las e voltar a chamar).
    ToolCalls,
    /// Teto de tokens de saída atingido.
    Length,
    /// Filtro de conteúdo.
    ContentFilter,
    /// Outro motivo reportado pelo endpoint (texto cru).
    Other(String),
}

/// Evento incremental do stream do modelo (só o que é visível ao modelo/loop).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ProviderEvent {
    /// Texto incremental (delta).
    Text(String),
    /// Raciocínio incremental (thinking), quando o endpoint o expõe.
    Thinking(String),
    /// Tool call **completa** (acumulada; paridade com o GDK).
    ToolCall {
        /// Identificador da chamada (do provider, preservado no log).
        call: CallId,
        /// Nome ao modelo da tool pedida.
        name: String,
        /// Argumentos já decodificados como JSON.
        arguments: Value,
    },
}

/// Resultado final de uma chamada ao modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutcome {
    /// Contabilização, quando o endpoint a reporta.
    pub usage: Option<TokenUsage>,
    /// Motivo de paragem.
    pub stop: StopReason,
}

/// Controlo de fluxo devolvido por um [`ProviderSink`] (cancelamento imediato).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Continuar a consumir.
    Continue,
    /// Parar já (o adaptador fecha a ligação).
    Break,
}

/// Consumidor de eventos do stream. Implementado pelo loop (E12-T05/E10) e pelos testes.
pub trait ProviderSink {
    /// Recebe um evento; devolve [`Flow::Break`] para cancelar.
    fn on_event(&mut self, event: ProviderEvent) -> Flow;
}

/// Implementação trivial que só mantém o último delta (útil em testes e sondagens).
#[derive(Debug, Default)]
pub struct CollectSink {
    /// Texto acumulado.
    pub text: String,
    /// Raciocínio acumulado.
    pub thinking: String,
    /// Tool calls completas, na ordem de chegada.
    pub calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for CollectSink {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        match event {
            ProviderEvent::Text(delta) => self.text.push_str(&delta),
            ProviderEvent::Thinking(delta) => self.thinking.push_str(&delta),
            ProviderEvent::ToolCall {
                call,
                name,
                arguments,
            } => self.calls.push((call, name, arguments)),
        }
        Flow::Continue
    }
}

/// Porta de acesso a um endpoint de modelo.
///
/// `stream` consome o stream e devolve o resultado final. A implementação **não** bufferiza a
/// resposta inteira (latência primeiro): emite deltas à medida que chegam.
pub trait Provider: Send + Sync {
    /// Identificador estável do provider (ex.: `"opencode"`, `"llama"`, `"fake"`).
    fn id(&self) -> &str;

    /// Consome o stream do modelo, emitindo eventos no `sink`.
    ///
    /// # Errors
    /// [`ProviderError`] em falha de transporte, HTTP, decodificação ou cancelamento.
    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError>;
}

/// Erro de um provider (tipado; mapeia para a taxonomia do katu).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// Falha de transporte (socket, TLS, DNS).
    #[error("transporte do provider: {0}")]
    Transport(String),
    /// Resposta HTTP não-2xx, com o corpo (limitado) para diagnóstico.
    #[error("HTTP {status} do provider: {body}")]
    Http {
        /// Código de estado.
        status: u16,
        /// Corpo da resposta (truncado).
        body: String,
    },
    /// Resposta que não segue o protocolo esperado.
    #[error("resposta inválida do provider: {0}")]
    Decode(String),
    /// O modelo pediu uma tool fora do catálogo (fail-closed).
    #[error("tool desconhecida: {0}")]
    UnknownTool(String),
    /// Dialeto/capacidade ainda não suportado por este adaptador.
    #[error("provider não suportado: {0}")]
    Unsupported(String),
    /// Cancelado pelo consumidor ([`Flow::Break`]).
    #[error("chamada ao provider cancelada")]
    Cancelled,
    /// Tempo esgotado.
    #[error("timeout de {millis} ms no provider")]
    Timeout {
        /// Limite em milissegundos.
        millis: u64,
    },
}

impl From<ProviderError> for Error {
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::Timeout { millis } => Self::Timeout {
                operation: "provider",
                millis,
            },
            ProviderError::Unsupported(message) => Self::Config { message },
            ProviderError::Decode(message) | ProviderError::Transport(message) => {
                Self::Schema { message }
            }
            ProviderError::Http { status, body } => Self::Unavailable {
                service: format!("provider (HTTP {status}): {body}"),
            },
            ProviderError::UnknownTool(name) => Self::InvalidInput {
                message: format!("tool desconhecida: {name}"),
            },
            ProviderError::Cancelled => Self::Conflict {
                message: "chamada ao provider cancelada".to_string(),
            },
        }
    }
}
