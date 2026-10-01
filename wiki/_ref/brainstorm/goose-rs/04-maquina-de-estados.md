# 04 — A máquina de estados (`goose-agent`)

A peça arquitetural mais interessante do goose é a extração do loop do agente para um **crate genérico reutilizável** (`goose-agent`, "The GDK's Agent Loop") com uma máquina de estados orientada a operações. É o destino da migração iniciada em `GOOSE_STATE_MACHINE=1`.

## 1. Estrutura do crate

`crates/goose-agent/src/`:

| Arquivo | LOC | Conteúdo |
|---|---:|---|
| `inference.rs` | 642 | `InferenceRunner`, `InferenceRequestPreparer`, `InferenceEffect`, spans |
| `tool.rs` | 371 | `ToolProvider`, `ToolOperation` |
| `operation.rs` | 270 | Traits `Operation`/`Inference`, `StepResult`, `OperationResult`, efeitos |
| `machine.rs` | 204 | `StateMachine`, `Step`, traits de runtime |
| `events.rs` | 18 | `AgentEvent` |
| `lib.rs` | 5 | Reexports |

Dependências: apenas `goose-provider-types`, `rmcp`, `tokio`, `futures`, `anyhow`, `uuid`, `tracing`. **Não conhece** os detalhes do goose de aplicação.

## 2. O modelo conceitual

```
        ┌─────────────────────────────────────────────┐
        │ StateMachine<'a, S, E>                       │
        │   steps: Vec<Step>   cancel: CancellationToken│
        └───────────────────┬─────────────────────────┘
                            │ step()
        ┌───────────────────▼─────────────────────────┐
        │ for step in steps {                          │
        │   Operation → OperationResult                │
        │   Inference → OperationResult                │
        │ }                                            │
        └───────────────────┬─────────────────────────┘
                            │ Applied(StepResult<E>)
        ┌───────────────────▼─────────────────────────┐
        │ apply() → EffectHandler.apply_effects()      │
        └─────────────────────────────────────────────┘
```

- `S` é o tipo de **sessão** (ex.: `Session`).
- `E` é o tipo de **efeito** (ex.: `GooseEffect`, ou o genérico `ConversationEffect`).
- O estado vive na conversa persistida; a máquina é **re-entrante** — ela recarrega a sessão a cada passo.

## 3. Traits do runtime

```rust
pub trait MachineSession: Send + Sync {
    fn id(&self) -> &str;
    fn conversation(&self) -> Option<&Conversation>;
}

#[async_trait]
pub trait SessionLoader<S>: Send + Sync {
    async fn load(&self, session_id: &str) -> Result<S>;
}

#[async_trait]
pub trait EffectHandler<S, E>: Send + Sync {
    async fn apply_effects(&self, session: &S, effects: &mut [E], emit: &Emitter) -> Result<()>;
}

pub trait EffectUsage<E>: Send + Sync {
    fn usage(&self, _effect: &E) -> Option<Usage> { None }
}
```

O `SessionManager` implementa `SessionLoader<Session>` **e** `EffectHandler<Session, GooseEffect>`. Ou seja: a persistência e a aplicação de efeitos são o mesmo subsistema.

## 4. `Step`, `Operation`, `Inference`

```rust
pub enum Step<'a, S, E = ConversationEffect> {
    Operation(Arc<dyn Operation<S, E> + 'a>),
    Inference(Arc<dyn Inference<S, E> + 'a>),
}
```

### Trait `Operation`

```rust
#[async_trait]
pub trait Operation<S, E: Send + 'static = ConversationEffect>: Send + Sync {
    fn name(&self) -> &'static str;

    // Anota na mensagem o que a operação fez, para que um pipeline
    // reconstruído da conversa persistida chegue à mesma conclusão.
    fn set_message_meta(&self, message: &mut Message, key: &str, value: serde_json::Value);
    fn message_meta<'a>(&self, message: &'a Message, key: &str) -> Option<&'a serde_json::Value>;

    async fn cancel(&self, ...) -> Result<OperationResult<E>> { ok(result) }
    async fn run_command(&self, ...) -> Result<OperationResult<E>> { not_applicable() }
    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> { Ok(vec![]) }
    async fn prompt_parts(&self, ...) -> Result<Vec<(String, String)>> { Ok(vec![]) }
    async fn moim_parts(&self, ...) -> Result<Vec<String>> { Ok(vec![]) }
    async fn run(&self, ...) -> Result<OperationResult<E>> { not_applicable() }
}
```

### Trait `Inference`

```rust
#[async_trait]
pub trait Inference<S, E>: Operation<S, E> {
    fn applies(&self, conversation: &Conversation) -> bool;
    async fn infer(&self, session, conversation, input: InferenceInput, emit) -> Result<OperationResult<E>>;
}
```

### O insight do `set_message_meta`

> "Note on a message something this operation did, so that a pipeline rebuilt from the persisted conversation reaches the same conclusion. Notes record past actions only — anything an operation would have to compute again does not belong here."

Isso é **determinismo reconstruível**: como a máquina recarrega a conversa a cada passo, uma operação precisa registrar seu efeito de forma idempotente para não refazê-lo. Metadados marcam "isto já foi feito", nunca "recalcule isto".

## 5. Resultado e efeitos

```rust
pub struct StepResult<E> {
    pub effects: Vec<E>,
    pub applied_step: Option<&'static str>,
    pub yield_to_client: bool,
}
pub enum OperationResult<E> {
    NotApplicable,
    Applied(StepResult<E>),
}
```

Helpers: `not_applicable()`, `applied(effects)`, `yielded()`, `yielded_with(effects)`.

Efeitos genéricos de conversa:

```rust
pub enum ConversationEffect {
    AppendMessage(Message),
    ReplaceConversation(Conversation),
    PatchToolRequestMeta { tool_call_id, patch },
    SetMessageVisibility { message_id, user_visible, agent_visible },
}
```

`MachineEffect::ensure_message_ids()` atribui IDs a mensagens sem ID — garantindo identidade estável para streaming e eventos.

## 6. O `Emitter`

```rust
pub struct Emitter { tx: mpsc::Sender<AgentEvent>, cancel: CancellationToken }
impl Emitter {
    pub async fn emit(&self, event: AgentEvent);
    pub async fn message(&self, message: Message) -> Message; // gera id + emite
    pub fn cancel_token(&self) -> &CancellationToken;
    pub async fn cancelled(&self);
}
```

## 7. Como `step()` funciona (lógica exata)

1. Obtém a conversa da sessão (erro se ausente).
2. **Para cada step na ordem:**
   - Se `cancel.is_cancelled()` → `NotApplicable` (pula).
   - `Operation` → `operation.run(session, conversation, emit)`.
   - `Inference` → se `!applies(conversation)` → `continue`; senão:
     - coleta `tools`, `prompt_parts`, `moim_parts` de **todas** as operações do pipeline (com `tokio::select!` com cancelamento);
     - rejeita **nomes de tool duplicados** entre operações (`multiple operations registered tool '...'`);
     - chama `inference.infer(...)`.
   - Se cancelou durante a execução → `operation.cancel(...)`.
   - `NotApplicable` → próximo step.
   - `Applied` → marca `applied_step`, garante IDs, se cancelado força `yield_to_client = true`, **retorna**.
3. Se nenhum step aplicou → `None` (fim do turno).

## 8. `run()` e o loop persistente

```rust
pub async fn run<R>(&self, runtime: &R, session_id: &str, emit: &Emitter) -> Result<S>
where R: SessionLoader<S> + EffectHandler<S, E> {
    loop {
        let session = runtime.load(session_id).await?;   // recarrega do storage
        let Some(mut result) = self.step(&session, emit).await? else { break };
        self.apply(runtime, &session, &mut result, emit).await?;
        if result.yield_to_client { break; }
    }
    runtime.load(session_id).await
}
```

O goose tem uma variante (`agents/state_machine/session.rs::run`) que adiciona telemetria (`gen_ai.*`), acúmulo de `turn_usage` e extração do último texto do assistente.

## 9. O pipeline concreto do goose (ordem exata)

Construído em `Agent::create_state_machine` (`agent.rs:1644`). A ordem importa: é a **primeira operação aplicável** que vence.

| # | Operação | Responsabilidade |
|---:|---|---|
| 1 | `EntryHookOperation` | Dispara hooks de início de turno |
| 2 | `SlashCommandOperation` | Despacha comandos `/...`; internamente delega a **todas** as outras operações + `StatusOperation` |
| 3 | `SteerOperation` | Consome a fila de "steer" do usuário (mensagens injetadas) |
| 4 | `MaxTurnsOperation` | Impõe limite de turnos |
| 5 | `BangShellOperation` | Executa comandos `!cmd` diretos |
| 6 | `CompactionOperation` | Compacta o contexto (se provider não gerencia o próprio) |
| 7 | `ToolPairCompactionOperation` | Sumariza pares tool-request/tool-response antigos |
| 8 | `ToolApprovalOperation` | Aplica modos de permissão |
| 9 | `DoctorOperation` | Diagnósticos (`/doctor`) |
| 10 | `ProjectOperation` | Troca/associa projeto (`/project`) |
| 11 | `SkillOperation` | Ativa skills |
| 12 | `RecipeOperation` | Executa/instancia recipes |
| 13 | `ToolExecutionOperation` | Executa as tool calls pendentes |
| 14 | `UnknownToolOperation` | Trata tools desconhecidas (devolve erro ao modelo) |
| 15 | `RetryOperation` | Retry/on-failure de recipe |
| 16 | `StopHookOperation` | Hooks de parada |
| 17 | `ExitOnErrorOperation` | Encerra em erro terminal |
| 18 | `Inference` (`InferenceRunner`) | Chama o provider quando nada mais se aplica |

`StatusOperation` é anexado apenas ao `SlashCommandOperation`.

## 10. `InferenceRunner` e preparação de request

- `InferenceRequestPreparer<S>` permite customizar o request antes do provider. O goose usa `GooseInferenceRequestPreparer` (monta system prompt, tools, MOIM).
- `InferenceEffect: From<Message>` permite que a inferência produza eventos genéricos.
- Spans de telemetria: `chat_span`, `record_chat_usage`, e os atributos `gen_ai.*`.

## 11. Por que isso é uma boa arquitetura

1. **Pipeline plugável** — adicionar comportamento = inserir uma `Operation` na ordem certa, não editar um `if` gigante.
2. **Reentrância** — recarregar a sessão a cada passo torna cada operação testável isoladamente e o sistema retomável.
3. **1 operação aplicável por passo** — sem ambiguidade sobre "quem decidiu".
4. **Efeitos explícitos** — toda mutação da conversa é um valor (`ConversationEffect`), aplicável de forma transacional pelo `EffectHandler`.
5. **Determinismo reconstruível** — `set_message_meta` evita recomputar decisões já tomadas.
6. **Cancelamento embutido** — `CancellationToken` participa de todas as fases, inclusive na coleta de tools.

## 12. Ponto de atenção

O pipeline é reconstruído a cada `reply`, com `Arc<dyn Operation>` referenciando dados do `Agent`. As operações guardam referências (`&'a`) a campos do agente (ex.: `&self.current_goose_mode`), o que explica o lifetime `StateMachine<'a, ...>`. Isso torna a máquina **não destacável** do `Agent` sem cuidado — uma restrição de design a considerar se `katu` quiser orquestração ainda mais desacoplada.
