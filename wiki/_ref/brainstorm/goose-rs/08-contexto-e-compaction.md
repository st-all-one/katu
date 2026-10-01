# 08 — Contexto e compaction

A gestão de contexto é uma das maiores fontes de complexidade e custo em agentes. O goose trata isso como um subsistema de primeira classe, genericamente extraído para o crate **GDK** `goose-context-management` e usado pelo pipeline (`CompactionOperation`, `ToolPairCompactionOperation`).

## 1. Arquitetura em três camadas

O cabeçalho de `goose-context-management/src/lib.rs` define a estratificação:

> "Three layers, smallest first:
> 1. `summarize` — given a model and messages, produce one summary message.
> 2. `compact` — the trait-based API (`CompactionInput`/`CompactionOutput`) letting a caller read from and write back to its own conversation representation. Rust only.
> 3. Cross-language (Python/Kotlin) access is exposed by `goose-sdk`, which wraps this crate in its uniffi bindings."

```rust
pub trait CompactionInput  { fn messages(&self) -> Vec<Message>; fn templates(&self) -> Templates; }
pub trait CompactionOutput { fn set_summary(&mut self, summary: Message); fn set_usage(&mut self, usage: ProviderUsage); }

pub async fn compact<I, O>(model, estimator, input, output) -> Result<()> { ... }

pub const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.8;
```

O ponto-chave é que o crate **não conhece a representação de conversa** do chamador — ele opera sobre `CompactionInput`/`CompactionOutput`.

## 2. Modelo e estimador

```rust
pub trait CompactionModel: Send + Sync { /* LLM que produz o resumo */ }
pub trait TokenEstimator:   Send + Sync { /* estima tokens */ }
pub struct ProviderModel { /* implementa CompactionModel sobre um Provider */ }
```

- `GooseCompactionModel` e `GooseTokenEstimator` são as impls do goose.
- `TokenCounter` (`goose/src/token_counter.rs`) mantém um **cache** de contagem com `count_tokens`, `count_tokens_for_tools`, `count_chat_tokens`, `count_everything`, `clear_cache`, `cache_size`.

## 3. Resumo estruturado (e tolerante)

O resumo não é texto livre — é um objeto estruturado (`StructuredSummary`):

```rust
pub struct StructuredSummary {
    pub user_intent: Vec<String>,
    pub technical_concepts: Vec<String>,
    pub files: Vec<FileActivity>,
    pub errors_and_fixes: Vec<String>,
    pub problem_solving: Vec<String>,
    pub user_messages: Vec<String>,
    pub pending_tasks: Vec<String>,
    pub current_work: Option<String>,
    pub next_step: Option<String>,
    #[serde(flatten)] pub extra: serde_json::Map<String, serde_json::Value>,
}
```

Decisões de robustez documentadas no código:

> "Every list is ordered most-important-first so consumers ... can cut from the tail. Fields deserialize leniently — omitted fields default to empty, and an object or number where a string was asked for is stringified rather than failing — because models routinely enrich the schema and one such field must not discard a good summary."

Isso é engenharia defensiva *no lugar certo*: a saída do LLM é inerentemente ruidosa, então o parser é tolerante, mas o modelo é tipado. `extra` (flatten) preserva campos desconhecidos para customizações.

O render usa **MiniJinja** (`templates.rs`), com `Templates` padrão e `builtin_template`.

## 4. `CompactingProvider` — compaction como wrapper

`goose-context-management/src/provider.rs` implementa compaction **decorando o provider**:

```rust
/// Wraps a provider so a `ContextLengthExceeded` response triggers compaction
/// and one retry with the summary standing in for the prior history.
pub struct CompactingProvider { inner: Arc<dyn Provider>, templates: Templates }
```

Quando o provider devolve `ContextLengthExceeded`, o wrapper compacta e **retenta uma vez** com o resumo no lugar do histórico. É uma defesa em profundidade, independente da compaction proativa do pipeline.

## 5. As duas operações de compaction no pipeline

### 5.1 `CompactionOperation` (contexto inteiro)

- Dispara quando `tokens / context_limit > threshold` (default `0.8`, configurável por `GOOSE_AUTO_COMPACT_THRESHOLD`).
- Soma tokens "não reportados" de tool calls recentes (`unreported_tool_tokens`, `count_context_tokens`).
- Ao compactar: **marca mensagens antigas como `agent_visible: false`** (mas `user_visible: true`) e **anexa um resumo**. Não deleta nada.
- Emite o efeito `GooseEffect::CompactConversation { conversation, usage }`.
- Pode injetar um aviso de budget no contexto: `<compaction>~{n}k tokens remaining</compaction>`.

### 5.2 `ToolPairCompactionOperation` (pares tool-request/response)

- Calcula um cutoff: `compute_tool_call_cutoff(context_limit, compaction_threshold)` ou `GOOSE_TOOL_CALL_CUTOFF`.
- Identifica pares antigos via `tool_ids_to_summarize` e os resume com `summarize_tool_call`.
- Esconde (`agent_visible: false`) request+response e anexa o resumo.
- Ativável por `tool_pair_summarization_enabled()`; desabilitado se o provider "gerencia o próprio contexto" (`manages_own_context()`).

## 6. O princípio: compactação não é exclusão

Confirmado em código (`ops_compaction.rs`):

```rust
last.metadata.agent_visible = false;   // sai do contexto do modelo
// user_visible permanece true            // continua no transcript
```

E no teste `live_replacement_preserves_concurrent_transcript_and_hidden_handoff`, que verifica que uma mensagem "concorrente" adicionada durante a compaction **é preservada** mesmo após `save_compacted_conversation`.

Ou seja: a compaction altera **o que o agente vê**, não o que foi registrado. É o mesmo princípio de "não deletar" do Pi, com uma implementação diferente (flag de visibilidade em vez de entradas separadas).

## 7. Limite de contexto

`goose-provider-types/src/context_limit.rs` e `Provider::get_context_limit`:

```rust
async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize
```

Resolução em cascata: override do consumidor → configuração do provider → metadados canônicos → default global. O método é **infalível** por design (sempre cai para um valor).

## 8. MOIM e o "turn context"

O **MOIM** (Model-Observed Internal Memory) injeta contexto operacional a cada turno:

- Constante `MIN_CONTEXT_FOR_MOIM = 32_000`.
- Bloco `<turn-context>` gerado por turno (hora atual, diretório, **status de compaction**, budget de turno, contexto de extensões).
- `compute_compaction_info(session_id, extension_manager)` informa ao modelo o estado da compaction atual.

`MessageMetadata.turn_context: true` marca essas mensagens.

## 9. Configuração relevante

| Variável | Default | Efeito |
|---|---|---|
| `GOOSE_AUTO_COMPACT_THRESHOLD` | 0.8 | Fração do contexto que dispara compaction |
| `GOOSE_TOOL_CALL_CUTOFF` | derivado | Nº de tool calls antes de sumarizar pares |
| `GOOSE_MOIM_MESSAGE_TEXT` / `GOOSE_MOIM_MESSAGE_FILE` | — | Instruções persistentes no MOIM |
| `tool_pair_summarization` | habilitado | Sumarização de pares |

## 10. Lições

1. **Extrair compaction para um crate genérico** com traits de input/output desacopla-a da representação de conversa e habilita bindings cross-language.
2. **Resumo estruturado e tolerante**: modelo tipado + parse leniente = robustez contra saída de LLM.
3. **Compactação por visibilidade, não por exclusão**: preserva o transcript e a auditoria.
4. **Duas escalas de compaction**: contexto inteiro e pares tool; a segunda é mais barata e frequente.
5. **Wrapper de provider (`CompactingProvider`)** para retry em `ContextLengthExceeded` — defesa em profundidade.
6. **Estimador de tokens com cache** evita recomputação.
7. **Injeção de contexto por turno (MOIM)** mantém o modelo orientado sem inflar o prompt de sistema.
