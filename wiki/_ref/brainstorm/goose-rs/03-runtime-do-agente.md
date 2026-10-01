# 03 — Runtime do agente

## 1. Onde vive

O núcleo está em `crates/goose/src/agents/`:

| Arquivo | Bytes | Papel |
|---|---:|---|
| `agent.rs` | ~252 KB (6.164 linhas) | Struct `Agent`, `reply`, loop legado, gestão de provider/extension/hooks |
| `reply_parts.rs` | ~78 KB (2.123 linhas) | Partes do turno: montagem de prompt, execução de tools, confirmações |
| `extension_manager/mod.rs` | ~98 KB (2.782 linhas) | Ciclo de vida das extensões MCP |
| `mcp_client.rs` | ~58 KB | Cliente MCP unificado |
| `state_machine/*` | vários | Pipeline novo orientado a operações |
| `tool_schema_normalize.rs` | ~38 KB | Normalização de schemas de tools por provider |
| `retry.rs` | ~18 KB | Política de retry |
| `tool_confirmation_*.rs` | ~13 KB | Confirmação de uso de tool |

## 2. O struct `Agent`

Construído via `Agent::new()` (CLI) ou `Agent::with_config(AgentConfig)`. Campos relevantes (extraídos do construtor, `agent.rs:403`):

```rust
pub struct Agent {
    provider: Arc<Mutex<Option<Arc<dyn Provider>>>>,   // trocável em runtime
    config: AgentConfig,
    current_goose_mode: Mutex<GooseMode>,              // auto|approve|smart_approve|chat
    extension_manager: Arc<ExtensionManager>,
    final_output_tool: Arc<Mutex<Option<...>>>,
    prompt_manager: Mutex<PromptManager>,
    tool_confirmation_router: ToolConfirmationRouter,
    tool_confirmation_coordinator: ToolConfirmationCoordinator,
    retry_manager: RetryManager,
    tool_inspection_manager: ...,
    hook_manager: HookManager,
    session_start_emitted: AtomicBool,
    container: Mutex<Option<Container>>,               // execução de extensões em Docker
    goal: Mutex<Option<String>>,
    grind: Mutex<Option<String>>,
    steer_queues: Mutex<HashMap<String, SteerQueue>>,  // mensagens "empurradas" mid-turn
}
```

Pontos de design:

- **O provider é mutável em runtime** (`Mutex<Option<Arc<dyn Provider>>>`) — permite trocar provider/modelo no meio da sessão.
- **`GooseMode`** governa permissões: `Auto` (aprova tudo), `Approve` (pergunta sempre), `SmartApprove` (só sensível), `Chat` (sem tools).
- **`steer_queues`** por sessão: o usuário pode injetar mensagens enquanto o agente trabalha.
- O `Agent` é **clonável via `Arc`** e reutilizado por sessão; a sessão é identificada por `session_id`.

## 3. O contrato de saída: `AgentEvent`

Definido em `goose-agent/src/events.rs` e reexportado:

```rust
pub enum AgentEvent {
    Message(Message),
    Usage(ProviderUsage),
    MessageUsage { message_id: Option<String>, usage: MessageUsage },
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
}
```

`Agent::reply(...)` devolve:

```rust
pub async fn reply(
    &self,
    user_message: Message,
    session_config: SessionConfig,
    use_state_machine: bool,
    cancel_token: Option<CancellationToken>,
) -> Result<BoxStream<'_, Result<AgentEvent>>>
```

Ou seja, a interface do agente é **um stream de eventos**, não um valor de retorno único. Isso é o que permite CLI, desktop, ACP e HTTP compartilharem a mesma lógica e apenas renderizarem diferente.

O `reply` envolve o stream num `ensure_message_event_id` — "the single live-event identity boundary" — garantindo que eventos de uma mesma mensagem lógica compartilhem ID.

## 4. Dois caminhos de execução (migração em curso)

O `AGENTS.md` documenta a migração explicitamente:

> "We are replacing the legacy agent loop in `crates/goose/src/agents/agent.rs` with the state machine in `crates/goose/src/agents/state_machine/`. The state-machine path is enabled with `GOOSE_STATE_MACHINE=1`. Until the migration is complete, changes to agent-loop behavior must be implemented and tested in both paths."

### 4.1 Loop legado — `reply_impl`

Um método grande que, a cada `reply`:

1. Processa a mensagem do usuário (incluindo `ActionRequired` / elicitation).
2. Persiste a mensagem na sessão.
3. Monta o prompt de sistema + lista de tools (com `toolshim` quando o modelo não suporta tools nativas).
4. Chama `provider.stream(...)` e consome o stream, emitindo `AgentEvent`s.
5. Acumula `tool_requests` e executa (`dispatch_tool_call`), com confirmação quando necessário.
6. Trata erros como conteúdo (`Repair`), aplica compaction, retry, hooks.
7. Repete até não haver mais tool calls pendentes (ou limite de turns).

Esse caminho embute **muita política** diretamente em `agent.rs`, o que motivou a migração.

### 4.2 Máquina de estados — `reply_with_state_machine`

Delega para o crate `goose-agent`:

1. Persiste a mensagem do usuário.
2. Constrói uma `StateMachine` com uma **lista ordenada de operações** + uma operação de inferência terminal (`create_state_machine`, `agent.rs:1644`).
3. Roda o pipeline: cada passo recarrega a sessão, escolhe a **primeira operação aplicável**, aplica efeitos e, se `yield_to_client`, para.
4. Emite eventos; o loop externo consome.

A escolha é feita por flags:

- Global: `GOOSE_STATE_MACHINE=1`.
- Por requisição ACP: via `meta` (`use_state_machine_from_meta`, `acp/server.rs:421`).

## 5. Detalhes de comportamento observáveis

- **Retomada**: como o estado é sempre persistido, um `reply` interrompido pode ser retomado recarregando a sessão.
- **Confirmações de tool**: coordenadas por `ToolConfirmationCoordinator` + `ToolConfirmationRouter`; a confirmação é representada como `ActionRequired` dentro da conversa, não como estado externo.
- **Subagentes/delegação**: `subagent_handler.rs` + `subagent_execution_tool/` permitem que o agente delegue tarefas, com `TaskConfig` e prompts próprios.
- **`final_output_tool`**: um "tool sintético" injetado para forçar saída estruturada (JSON schema) quando desejado.
- **`tool_inspection_manager`**: pipeline de inspectors (permissão, segurança/repetição) que decide aprovar/negar/alertar antes de executar.

## 6. Lições

1. **Interface de agente = stream de eventos.** Desacopla lógica de UI.
2. **Provider trocável em runtime.** Um `Mutex<Option<Arc<dyn Provider>>>` é suficiente e flexível.
3. **Migração incremental de loop monolítico → pipeline de operações** com flag de ambiente e requisito de paridade nos dois caminhos.
4. **Persistir a cada passo** torna o agente retomável e o estado auditável.
5. **Confirmação/permissão como dado da conversa** (`ActionRequired`), não como estado paralelo.
