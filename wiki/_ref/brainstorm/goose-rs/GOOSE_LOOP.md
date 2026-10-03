# GOOSE_LOOP — loop, turnos, sessão e I/O do modelo (goose)

> **O que é.** Mergulho de código no **controlo do loop** do goose: onde começa um turno, onde
> acaba, como a sessão é recarregada, e como o pedido/resposta do modelo são montados e
> descodificados. É o complemento operacional de
> [`03 — Runtime do agente`](03-runtime-do-agente.md),
> [`04 — Máquina de estados`](04-maquina-de-estados.md),
> [`05 — Providers`](05-providers.md),
> [`07 — Sessão e storage`](07-sessao-e-storage.md) e
> [`09 — CLI`](09-cli.md): aqui não se repete a arquitectura, mapeia-se **peça a peça o que o loop
> faz com ela**.

**Alvo:** `_REF/goose/` · commit `4dea9b4` · workspace `1.52.0`.
**Método:** leitura directa do código; cada afirmação tem `ficheiro:linha`.

---

## 0. Sumário em uma frase

O goose tem **um único ponto de entrada** (`Agent::reply`) que devolve um **stream de eventos**
(`AgentEvent`) e tem **dois motores** por trás da mesma interface: um **loop legado monolítico**
(`reply_internal`) e uma **máquina de estados re-entrante** (`goose-agent`) activada por
`GOOSE_STATE_MACHINE=1`. Ambos operam sobre a **mesma sessão persistida em SQLite** e o mesmo
contrato de provider (`Provider::stream` → `MessageStream`). O que muda é **quem decide o próximo
passo**: um `if` gigante, ou uma lista ordenada de operações em que vence a primeira aplicável.

---

## 1. Vocabulário operacional (código, não analogia)

| Termo | Tipo | Onde | Definição operacional |
|---|---|---|---|
| **Mensagem** | `Message` | `goose-provider-types/src/conversation/message.rs` | `{ id, role, created, content: Vec<MessageContentBlock>, metadata }` |
| **Conteúdo** | `MessageContentBlock` | idem | união: `Text`, `Image`, `ToolRequest`, `ToolResponse`, `ActionRequired`, `Thinking`, `SystemNotification`, `Error`… |
| **Metadados** | `MessageMetadata` | idem | `user_visible`, `agent_visible`, `inference`, `output_token_limit_reached`, `usage`, `operations` |
| **Conversa** | `Conversation(Vec<Message>)` | `conversation.rs:14` | lista append-only; `push` faz merge por `id` e trata `output_token_limit_reached` |
| **Sessão** | `Session` | `session/session_manager.rs:62` | linha mutável: `id`, `working_dir`, `conversation: Option<Conversation>`, `usage`, `recipe`, `goose_mode`… |
| **Config de sessão** | `SessionConfig` | `agents/types.rs:71` | `{ id, schedule_id, max_turns: Option<u32>, retry_config }` — o que o chamador passa a `reply` |
| **Evento** | `AgentEvent` | `goose-agent/src/events.rs:9` | `Message`, `Usage`, `MessageUsage`, `McpNotification`, `HistoryReplaced` |
| **Turno (agent turn)** | — | ver §5 | uma iteração do loop externo: **uma chamada ao provider + as tools que ela pede**, até a conversa terminar em assistente sem `ToolRequest` |
| **Passo (step)** | — | ver §4 | uma operação do pipeline da máquina de estados |

**Nota de ambiguidade:** "turno" no goose significa **iteração do agente**, não "mensagem do
utilizador". O orçamento `max_turns` limita quantas iterações o agente faz **sem input humano**.

---

## 2. Entrada única e dois motores

`Agent::reply` é a única porta. Todos os consumidores passam por ela:

```
CLI (session/mod.rs:1329) ─┐
ACP (acp/server.rs:2325)  ─┤
gateway (gateway/handler.rs:550) ─┼─► Agent::reply(agent.rs:2045)
scheduler (scheduler/full.rs:983) ┤       │
subagent (subagent_handler.rs:199)┘       ▼
                                    reply_impl(agent.rs:2071)
                                          │
                     ┌────────────────────┴────────────────────┐
                     ▼                                         ▼
        use_state_machine == true                  use_state_machine == false
        reply_with_state_machine(1771)             (legacy) reply_internal(2417)
        → run_goose(session.rs:169)                → loop externo + loop interno
```

- `reply` (`agent.rs:2045`) **não** decide o motor; recebe `use_state_machine: bool` do chamador.
  A CLI passa `state_machine::enabled()` (`mod.rs:1332`), que lê `GOOSE_STATE_MACHINE`
  (`state_machine/mod.rs:67`). O ACP pode decidir por pedido via `meta`.
- `reply` envolve o stream em `ensure_message_event_id` — a **fronteira de identidade** dos eventos
  vivos: eventos da mesma mensagem lógica partilham `id` (`agent.rs:2061`).
- `reply_impl` trata **antes** do loop: respostas a `elicitation` (devolve stream vazio),
  slash-commands (`/goal`, `/compact`, `/clear`…), hooks `UserPromptSubmit`, e a decisão do motor.
- `reply_impl` devolve `BoxStream<'_, Result<AgentEvent>>`. O loop do chamador é sempre o mesmo:
  `while let Some(event) = stream.next().await { render(event) }`.

---

## 3. Motor A — loop legado (`reply_internal`)

### 3.1 Preparação (`agent.rs:2417`–`2680`)

1. `prepare_reply_context` (`agent.rs:845`) devolve `ReplyContext` (`agent.rs:179`):
   `conversation`, `tools`, `toolshim_tools`, `system_prompt`, `goose_mode`, `tool_call_cut_off`,
   `model_config`. É aqui que o prompt de sistema e o catálogo de tools são montados
   (`prepare_tools_and_prompt`, `reply_parts.rs:198`).
2. `load_project_instructions` anexa `AGENTS.md` do projecto ao system prompt.
3. `provider.resume(saved_provider_session_id)` — se o provider mantém sessão própria
   (ex.: Anthropic/OpenAI stateful), retoma-a; senão é um no-op (`base.rs:505`).
4. Conta `pre_turn_tool_count` (tools já presentes **antes** deste reply) para não sumarizar o
   turno actual.
5. `initial_messages = conversation.messages().clone()` (`agent.rs:2578`) — snapshot usado pelos
   retries de recipe para preservar o prefixo enviado.

### 3.2 Estrutura: dois loops aninhados

```
loop externo (turnos)                      agent.rs:2588
  ├─ drena steers pendentes                (2589)
  ├─ aplica final_output pendente          (2612)
  ├─ turns_taken += 1  (com excepções)     (2664)
  ├─ if turns_taken > max_turns → MAX_TURNS_MESSAGE  (2674)
  ├─ stream = stream_response_from_provider(...)      (2680)
  │
  ├─ loop interno (chunks do provider)     (2737)
  │    ├─ Ok((response, usage)) → texto/thinking/tool_requests
  │    ├─ Err(ContextLengthExceeded) → compacta e recomeça
  │    ├─ Err(CreditsExhausted/Auth/…) → mensagem + break
  │    └─ break quando exit_chat ou fim do stream
  │
  ├─ decisão de fim de turno               (3288)
  ├─ persistência + conversation.extend    (3502)
  └─ exit_chat → stop hook → break / continuar
```

**Variáveis de estado do turno** (todas locais ao `try_stream!`, `agent.rs:2545`–`2565`):

| Variável | Papel |
|---|---|
| `turns_taken` / `max_turns` | orçamento; default `DEFAULT_MAX_TURNS = 1000` (`agent.rs:86`) |
| `compaction_attempts` | reset a 0 a cada chunk OK; corta após 2 (`agent.rs:3145`) |
| `empty_turn_retries` / `retrying_after_empty_turn` | retry de resposta vazia; cap `MAX_EMPTY_TURN_RETRIES = 3` (`agent.rs:89`) |
| `last_assistant_text` | último texto visível, para stop hooks e telemetria |
| `turn_total_usage` | soma de `ProviderUsage` do turno |
| `goal_check_pending`, `tool_pair_summarization_done`, `stop_hook_handled_for_exit`, `consecutive_stop_hook_blocks`, `can_drain_pending_steers` | flags de política |

**Variáveis do passo** (reset a cada iteração, `agent.rs:2710`–`2721`):
`no_tools_called`, `messages_to_add: Conversation`, `tools_updated`, `did_recovery_compact_this_iteration`,
`exit_chat`, `provider_errored`, `provider_produced_content`, `provider_reached_output_token_limit`,
`pending_final_output`, `pending_turn_usage`, `preferred_turn_usage_message_id`,
`surfaced_thinking_in_turn`.

### 3.3 Chamada ao provider

```rust
let mut stream = reply_parts::stream_response_from_provider(
    self.provider().await?, model_config.clone(), &session_config.id,
    &system_prompt, conversation.messages(), &tools, &toolshim_tools,
).await?;                                   // agent.rs:2680
```

O corpo de `stream_response_from_provider` está em `reply_parts.rs:342` e é o **pipeline de
entrada do modelo** (detalhado em §7.1).

### 3.4 Consumo do stream (loop interno, `agent.rs:2737`)

Cada item é `Result<(Option<Message>, Option<ProviderUsage>), ProviderError>`:

1. **Usage** → `update_session_metrics` (`reply_parts.rs:744`) → `AgentEvent::Usage`;
   acumula em `turn_total_usage`.
2. **`response.metadata.output_token_limit_reached`** → marca
   `provider_reached_output_token_limit` (o provider sinalizou `finish_reason == "length"`).
3. **Só `SystemNotification`** → emite e `continue` (não conta como conteúdo).
4. `provider_produced_content |=` há texto/imagem/thinking não vazio.
5. `categorize_tool_requests` (`reply_parts.rs:587`) separa `tool_requests` e filtra a resposta
   (coerção de argumentos, nomes deformados, remoção de thinking repetido).
6. Se `filtered_response` não vazio → `AgentEvent::Message`.
7. **Sem tool requests** → acumula texto em `last_assistant_text`, `messages_to_add.push(response)`,
   `continue`.
8. **Com tool requests** → executa (3.5) e `no_tools_called = false`.

### 3.5 Execução das tools (dentro do loop interno)

```
tool_requests
  → tool_inspection_manager.inspect_tools(...)        agent.rs:2862
  → process_inspection_results_with_permission_inspector(...)  (approved/needs_approval/denied)
  → handle_approved_and_denied_tools(...)             agent.rs:900
  → handle_approval_tool_requests(...)  (emite ActionRequired p/ UI)
  → stream::select_all(futuros) + loop de ToolStreamItem
       Result  → add_tool_response_with_metadata
       Message → AgentEvent::McpNotification
       ActionRequired → AgentEvent::Message (persiste)
  → por cada request: request_msg (assistant+ToolRequest) + final_response (user+ToolResponse)
       → messages_to_add.push(...) ; yield AgentEvent::Message(final_response)
```

Ponto fino: a resposta do provider com N tool calls é **dividida em N mensagens assistant** (uma por
call), cada uma com o thinking apropriado, para sobreviver a *providers* que exigem um tool call por
mensagem (`agent.rs:3040`–`3118`). Um tool call **inparseável** entra no histórico como
`unparseable_tool_call` (placeholder) e o erro real viaja no par `ToolResponse` — mantém a conversa
bem-formada em todos os formatadores (`agent.rs:3066`).

### 3.6 Decisão de fim de turno (`agent.rs:3288`–`3436`)

```
empty_response = no_tools_called && !exit_chat && !provider_errored
                 && !did_recovery_compact_this_iteration
                 && !provider_reached_output_token_limit
                 && !provider_produced_content && last_assistant_text.is_empty()
```

Se `empty_response`: descarta `messages_to_add` (nunca persistir assistente vazio — providers
estritos rejeitam). Senão, `empty_turn_retries = 0`.

Se `no_tools_called && !exit_chat`, decide por ordem:

1. `final_output_tool` chamado? `Some(Some(text))` → `pending_final_output`, `exit_chat = true`.
2. `Some(None)` → nudge `FINAL_OUTPUT_CONTINUATION_MESSAGE` e continua.
3. Recovery compact → continua.
4. Steers pendentes → continua (serão drenados).
5. Goal/Grind activo e ainda não verificado → injeta nudge (agent-only) e continua.
6. Caso contrário → `handle_retry_logic` (`agent.rs:814`):
   - `Retried` → repõe conversa a partir do prefixo e reinicia o loop.
   - `Skipped` + `empty_response` + último tool não sucedido → retry de resposta vazia (até 3) ou
     `EMPTY_TURN_MESSAGE` visível e `exit_chat = true`.
   - `MaxAttemptsReached(msg)` → persiste e `exit_chat = true`.
   - `Ok(_)` → `exit_chat = true`.

Depois: tool-pair summarization (assíncrona), `pending_final_output` → mensagem visível, anexa usage,
`add_message` por mensagem, `conversation.extend(messages_to_add)`.

### 3.7 Terminação

- `exit_chat && !has_pending_steers` → stop hooks (`emit_stop_hook_blocking`, `agent.rs:548`):
  - `Allow` → `break`.
  - `Deny` → injeta contexto de negação, `retrying_after_stop_hook_denial = true`, continua.
    Após `DEFAULT_STOP_HOOK_BLOCK_CAP = 8` (`agent.rs:87`) negações consecutivas, força o fim.
- `turns_taken > max_turns` → `MAX_TURNS_MESSAGE` (não é erro; é resposta visível).
- Cancelamento (`CancellationToken`) → `break` no select de cada `stream.next()`.

---

## 4. Motor B — máquina de estados (`goose-agent`)

### 4.1 O loop

`run_goose` (`state_machine/session.rs:169`) é a variante goose de `StateMachine::run`
(`machine.rs:158`). O invariante é o mesmo:

```rust
loop {
    let session = runtime.load(session_id).await?;          // recarrega do SQLite
    let Some(mut result) = machine.step(&session, emit).await? else { break };
    // acumula usage por efeito
    machine.apply(runtime, &session, &mut result, emit).await?;   // EffectHandler
    if result.yield_to_client { break; }
}
```

- **A sessão é recarregada a cada passo.** Não há estado em memória que sobreviva a um passo: o
  estado é a conversa persistida. É isto que torna o pipeline re-entrante e retomável.
- `machine.step` devolve `None` quando **nenhuma** operação é aplicável → o turno acabou.
- `yield_to_client = true` (ex.: `MaxTurnsOperation`, `ExitOnErrorOperation`, `StopHookOperation`)
  termina o turno imediatamente após aplicar efeitos.

### 4.2 `step()` — primeira operação aplicável vence (`machine.rs:76`)

```
para cada step na ordem:
    Operation → run(session, conversation, emit)
    Inference → se !applies(conversation) → continue
                senão: coleta tools + prompt_parts + moim_parts de TODAS as operações
                       (tokio::select! com cancelamento; rejeita tools duplicadas)
                       → infer(...)
    NotApplicable → próximo step
    Applied(result) → marca applied_step, garante ids, se cancelado força yield_to_client
                      → devolve Some(result)
```

### 4.3 Pipeline concreto (ordem = prioridade)

`create_state_machine` (`agent.rs:1644`). A ordem está descrita em
[`04 — Máquina de estados`](04-maquina-de-estados.md#9-o-pipeline-concreto-do-goose-ordem-exata);
os que **fecham o turno** são:

| Operação | Arquivo | Condição de aplicação | Efeito |
|---|---|---|---|
| `SteerOperation` | `ops_steer.rs` | há mensagens na `SteerQueue` e a conversa termina turno | injeta mensagens do utilizador |
| `MaxTurnsOperation` | `ops_maxturns.rs` | `assistant_turn_count >= max_turns` | `yielded_with([MAX_TURNS_MESSAGE])` |
| `ExitOnErrorOperation` | `ops_exit_on_error.rs` | `trailing_error(conversation).is_some()` | `yielded()` |
| `StopHookOperation` | `ops_stop_hook.rs` | `ends_turn(messages)` | corre hooks; allow → `yielded()`, deny → nudge |
| `InferenceRunner` (terminal) | `goose-agent/src/inference.rs:184` | `should_infer` (§4.4) | chama o provider |

### 4.4 Quando a inferência se aplica

`InferenceRunner::applies` (`inference.rs`) usa:

- `messages_since_kickoff` (`operation.rs:19`): da última mensagem **user visível e não-tool** até
  ao fim — o "turno actual".
- `should_infer` (`inference.rs:245`): a projecção para o provider termina num papel
  `User | Tool` e não há mensagem final vazia.
- `trailing_error` (`operation.rs:29`): se há erro terminal, não infere (o `ExitOnErrorOperation`
  fecha).

`messages_for_provider` (`inference.rs:193`) reconstrói o que o modelo vê: só `agent_visible`,
remove `ToolRequest` não respondidos de turnos anteriores, e opcionalmente remove mensagens vazias.

### 4.5 Turno na máquina de estados

- `assistant_turn_count` (`operation.rs:37`): conta **blocos contíguos** de mensagens assistant.
- `ends_turn` (`operation.rs:60`): última mensagem é assistant, sem erro e **sem** `ToolRequest`
  nem `ActionRequired`. É o critério de "resposta final".
- `MaxTurnsOperation` emite `MAX_TURNS_MESSAGE` quando `assistant_turn_count >= max_turns`.

---

## 5. O que é um "turno" — lado a lado

| Aspecto | Loop legado | Máquina de estados |
|---|---|---|
| Unidade | iteração do `loop` externo = 1 chamada `provider.stream` | 1 `step()` que devolve `Applied` |
| Fim do turno | `exit_chat` + stop hooks | `ends_turn` / `ExitOnError` / `MaxTurns` / `StopHook` |
| Orçamento | `turns_taken > max_turns` | `assistant_turn_count >= max_turns` |
| Retry de resposta vazia | `MAX_EMPTY_TURN_RETRIES = 3` | `InferenceRunner` emite `EMPTY_RESPONSE_MESSAGE` e `yielded_with` |
| Concorrência de turnos | sem lock por sessão | `try_start_turn()` por sessão (`agent.rs:1782`) |
| Persistência | `add_message` por mensagem no fim do passo | `EffectHandler` aplica `GooseEffect` (append/replace/patch) |
| Estado | variáveis locais do `try_stream!` | conversa persistida + `OperationNotes` (`set_message_meta`) |

O `try_start_turn` é uma diferença material: só a máquina de estados **serializa turnos por sessão**
(`tool_confirmation_coordinator.rs:37`); o legado não tem esse guard.

---

## 6. Sessão

### 6.1 Persistência

- `SessionManager` (`session_manager.rs:316`) é **ao mesmo tempo** `SessionLoader<Session>` e
  `EffectHandler<Session, GooseEffect>` (`state_machine/session.rs:41,49`): carregar e aplicar
  efeitos são o mesmo subsistema.
- `get_session(id, include_messages)` (`:432`) lê a linha + mensagens ordenadas por
  `(created_timestamp, id)`.
- `add_message` (`:455`) garante **ordenação monotónica**: `created = max(created, MAX(stored))`
  para que uma mensagem preparada antes e preenchida depois nunca se reordene para trás.
- `replace_conversation` (`:459`) usado em compaction e retry; `save_compacted_conversation`
  preserva mensagens concorrentes (ver teste em `state_machine/session.rs`).
- Schema e detalhes em [`07 — Sessão e storage`](07-sessao-e-storage.md).

### 6.2 Visibilidade

`MessageMetadata` tem **dois** booleanos (`user_visible`, `agent_visible`). A projecção para o
modelo é `Conversation::agent_visible_messages` (`conversation.rs:178`); a UI usa
`user_visible_messages` (`:187`). Isto permite:

- kickoff/compaction com mensagens que existem para auditoria mas não entram no contexto;
- nudges de goal/grind `agent_only`;
- respostas de confirmação `user_only`.

### 6.3 Criar / retomar / forkar

- **Criar**: `SessionManager::create_session` (`:420`); CLI em `session/builder.rs:460`.
- **Retomar** (`--resume`): `builder.rs:472` carrega a sessão existente; valida que provider/modelo
  não mudaram quando o provider gere o próprio contexto (`builder.rs:280`).
- **Fork**: copia o histórico para uma nova sessão (`builder.rs:159`).
- **Retomar turno interrompido** (máquina de estados): `resume_state_machine_turn`
  (`agent.rs:1835`) só actua se houver `pending_tool_confirmations` ou uma resposta de confirmação
  persistida por aplicar. Reconstrói o `ActiveTurnGuard`, registra os pedidos pendentes e volta a
  correr `stream_state_machine_turn`.
- **Sessão de provider**: `latest_provider_session_id` (`inference.rs:216`) lê a metadata de
  inferência mais recente e chama `provider.resume(...)`; se falhar, faz *handoff* (aviso, continua).

---

## 7. I/O do modelo

### 7.1 Pipeline de entrada (request)

`stream_response_from_provider` (`reply_parts.rs:342`):

```
conversation.messages()
  → Conversation::agent_visible_messages()          conversation.rs:178
  → fix_conversation(...)                            conversation.rs:235
  → merge_consecutive_messages_for_request(...)      conversation.rs:522
  → (se toolshim) convert_tool_messages_to_text(...)
  → provider.stream(&model_config, system, msgs, tools)   base.rs:510
```

`fix_messages` (`conversation.rs:273`) corre, **por esta ordem**, 9 transformações:

1. `merge_text_content_items` — junta blocos de texto consecutivos.
2. `trim_assistant_text_whitespace` — remove whitespace final do assistant.
3. `remove_empty_messages` — remove mensagens sem conteúdo.
4. `fix_empty_tool_results` — preenche tool results vazios.
5. `fix_tool_calling` — garante pares request/response bem-formados.
6. `merge_consecutive_messages` — funde mensagens consecutivas do mesmo papel.
7. `dedupe_signed_thinking` — remove thinking assinado duplicado.
8. `fix_lead_trail` — garante que começa e acaba em user.
9. `populate_if_empty` — último recurso.

`fix_conversation` aplica isto **só** às mensagens `agent_visible`, preservando as não-visíveis no
sítio (`conversation.rs:235`).

### 7.2 Contrato do provider

```rust
pub type MessageStream = Pin<Box<dyn Stream<
    Item = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>> + Send>>;

pub trait Provider: Send + Sync {
    fn get_name(&self) -> &str;
    async fn resume(&self, session_id: &str) -> Result<(), ProviderError> { Ok(()) }
    async fn stream(&self, model_config, system, messages, tools) -> Result<MessageStream, ProviderError>;
    // ... context limit, retry_config, fetch_model_info, manages_own_context
}
```

Um item pode ter **mensagem**, **usage**, ou ambos. Usage pode chegar **sem** mensagem (frames de
metadata), e o loop trata isso. Detalhes de providers em
[`05 — Providers`](05-providers.md).

### 7.3 Pipeline de saída (decode)

`stream_openai_compat` (`openai_compatible.rs:237`) lê a resposta HTTP linha a linha e delega em
`response_to_streaming_message` (`formats/openai.rs:1234`):

- Lê SSE (`data: ...`, `[DONE]`), faz parsing de chunks.
- **Acumula tool calls** por `index`: primeiro `id`+`name`, depois `arguments` em deltas
  (`openai.rs:1314`–`1460`). Só emite quando `finish_reason` chega.
- `finish_reason == "length"` → `output_token_limit_reached = true`, e cada tool call pendente é
  emitido como `Err(output_token_limit_tool_error(...))` em vez de executar com argumentos truncados.
- Emite `Thinking` (reasoning) separado de `Text`.
- Erro de JSON de argumentos → `ToolRequest` com `Err(ErrorData)`; nunca rebenta o run.

Não-streaming (`openai.rs:769`) usa `response_to_message` (`openai.rs:709`) e devolve
`stream_from_single_message`.

### 7.4 Categorização de tool requests

`categorize_tool_requests` (`reply_parts.rs:587`) faz, por request:

1. **Coerção** de argumentos contra o schema da tool (`coerce_tool_arguments`).
2. **Recuperação de nome deformado** (`recover_mangled_tool_name`) — GLM/Minimax.
3. **Rejeição de tools não anunciadas** (executáveis) — evita executar o que não foi oferecido.
4. **Dedup por id**, preservando a ordem do provider.

Na máquina de estados, o equivalente é `pending_advertised_tool_requests`
(`ops_toolcalling.rs:678`): `ToolDisposition::{Execute, Decline, ParseError}`, validado contra
`known_tools` e contra a nota `advertised_tools` gravada pela inferência
(`ops_llm.rs`, `ADVERTISED_TOOLS_NOTE`).

### 7.5 Erros

| Erro | Tratamento |
|---|---|
| `ContextLengthExceeded` | `compaction_attempts += 1`; compacta e reinicia; após 2, mensagem e `break` (`agent.rs:3123`) |
| `CreditsExhausted` | mensagem com `top_up_url`; `break` |
| `Authentication` / `ServerError` / `RequestFailed` | mensagem de erro; `break` |
| Erro **antes** do primeiro item do stream | retry com backoff (`reply_parts.rs:~380`), excepto se `provider.manages_own_context()` |
| Erro **depois** do primeiro item | sem retry (evita duplicar output) |

---

## 8. CLI / TUI

- `run_interactive` (`goose-cli/src/session/mod.rs:587`): REPL com `rustyline`; cada input passa por
  `handle_input` → `handle_message_input` (`:780`) → `push_message` + `process_agent_response`.
- `process_agent_response` (`:1301`): monta `SessionConfig`, chama `agent.reply(...)` e consome o
  stream num `tokio::select!` com cancelamento. Ramifica por `AgentEvent`:

| Evento | Acção na TUI |
|---|---|
| `Message` | detecta confirmação/elicitation, senão `render_message_streaming` (com buffer anti-flicker) |
| `Usage` | guarda para stats |
| `MessageUsage` | ignora |
| `McpNotification` | `handle_mcp_notification` (spinners) |
| `HistoryReplaced` | substitui `self.messages` (compaction) |

- **Confirmação de tool**: `find_tool_confirmation` → `prompt_tool_confirmation` → `submit_tool_confirmation`;
  em modo não-interactivo, auto-permite excepto `Approve`/`SmartApprove`, onde é erro.
- **Elicitation**: `collect_elicitation_input`; resposta volta por `agent.reply` (stream vazio que
  desbloqueia o tool em espera).
- **Interrupção**: `handle_interrupted_messages` (`:1646`) remove o que foi feito após o último user
  e mantém os pares request/response consistentes.
- **Headless**: `headless` (`:1289`) chama o mesmo caminho com `interactive = false`.
- Formatos de saída (`text`/`json`/`stream-json`) tratados no fim de `process_agent_response`.

---

## 9. Sequência completa de um turno (legado)

```
CLI                       Agent(reply)                Provider            Tools/MCP
 │  reply(user_msg) ───────►│
 │                          │ reply_impl: persiste user_msg, monta tools/prompt
 │                          │ reply_internal: prepare_reply_context
 │                          │
 │                          │ ── loop externo (turno) ──────────────────────────────
 │                          │ stream_response_from_provider ──►│
 │                          │                                  │ SSE chunks
 │  ◄── AgentEvent::Message ─┤ ◄── (Message, usage) ────────────┤
 │                          │ categorize_tool_requests
 │                          │ se ToolRequest:
 │                          │   inspect → permission → approval ─────────────►│
 │  ◄── Message(ActionRequired) ┤                            execute tool ──►│
 │  ─── submit_tool_confirmation ►│                          ◄── ToolResult ─┤
 │  ◄── Message(ToolResponse) ─┤ messages_to_add
 │                          │
 │                          │ (repete loop externo; provider vê o ToolResponse)
 │                          │ ...
 │                          │ provider devolve assistant sem tool requests
 │                          │ empty_response? no_tools_called → exit_chat
 │                          │ stop hooks → Allow
 │  ◄── AgentEvent::Message(final) ┤ persiste tudo, break
 │  ◄── stream end ─────────┤
```

Na máquina de estados, o mesmo turno é uma sequência de `step()` que aplicam
`ConversationEffect`s; o provider é o último `Step::Inference` e o turno fecha quando
`ends_turn` for verdadeiro e nenhuma operação se aplicar.

---

## 10. O que isto ensina (para o `katu`)

Mecanismos concretos que atacam exactamente os sintomas de "turnos/sessões/TUI que não dão certo":

1. **A conversa persistida é a máquina de estados.** Recarregar a sessão a cada passo (em vez de
   manter estado em memória) torna o turno retomável e o *replay* determinístico. O `katu` já tem
   `prompt_state`; o goose mostra o limite: **uma operação por passo, primeiro aplicável vence**
   (`04-maquina-de-estados.md`).
2. **Dois booleanos de visibilidade, não um.** `user_visible` vs `agent_visible` separa o que a UI
   mostra do que o modelo vê — resolve nudges/kickoffs e o "eco" de conteúdo interno na tela.
3. **Fim de turno é estrutural, não heurístico.** `ends_turn` = assistant sem `ToolRequest` e sem
   erro. Nada de inferir "acabou" por texto. Um turno só fecha quando a conversa o diz.
4. **Toda resposta vazia é tratada explicitamente.** `empty_response` nunca é persistida; há retry
   limitado e, no limite, uma mensagem visível. O utilizador nunca fica sem resposta.
5. **O provider sinaliza truncagem por tool call.** `finish_reason == "length"` transforma o tool
   call em erro em vez de executá-lo com JSON truncado — foi assim que o goose evita o sintoma de
   "o modelo devolve JSON/TOON cru".
6. **A entrada do modelo é normalizada por um pipeline fixo de 9 passos** (`fix_messages`), incluindo
   `fix_tool_calling` e `fix_lead_trail`. A ordem é a garantia de que *qualquer* provider recebe uma
   conversa bem-formada.
7. **Persistência com ordem monotónica** (`add_message`) evita reordenar mensagens preparadas antes e
   preenchidas depois — o bug clássico de tool call antes da resposta.
8. **Um turno por sessão** (`try_start_turn`) impede duas respostas concorrentes sobre a mesma
   conversa.
9. **Interface = stream de eventos.** A TUI não conhece o loop; só consome `AgentEvent`. Isso é o que
   permite CLI, desktop e ACP partilharem o mesmo motor.

> Comparação de fundo (o que copiar/evitar) em
> [`15 — Decisões e lições`](15-decisoes-e-licoes.md); a tese de loop do `katu` em
> [`05-kernel-loop.md`](../../plan/05-kernel-loop.md).

---

## 11. Referências de código

**Agente / loop**
`crates/goose/src/agents/agent.rs:86` (`DEFAULT_MAX_TURNS`), `:179` (`ReplyContext`), `:845`
(`prepare_reply_context`), `:900` (`handle_approved_and_denied_tools`), `:1067`
(`dispatch_tool_call`), `:1644` (`create_state_machine`), `:1771` (`reply_with_state_machine`),
`:1835` (`resume_state_machine_turn`), `:2045` (`reply`), `:2071` (`reply_impl`), `:2417`
(`reply_internal`), `:2545` (variáveis do turno), `:2588` (loop externo), `:2680` (chamada ao
provider), `:2737` (loop interno), `:3288` (fim de turno), `:3502` (persistência).

**Turno / parts**
`crates/goose/src/agents/reply_parts.rs:198` (`prepare_tools_and_prompt`), `:307`
(`prepare_tools_for_provider`), `:342` (`stream_response_from_provider`), `:587`
(`categorize_tool_requests`), `:744` (`update_session_metrics`).

**Máquina de estados**
`crates/goose-agent/src/machine.rs:76` (`step`), `:140` (`apply`), `:158` (`run`);
`crates/goose-agent/src/operation.rs:19` (`messages_since_kickoff`), `:29` (`trailing_error`),
`:37` (`assistant_turn_count`), `:60` (`ends_turn`), `:83` (`Operation`), `:141` (`Inference`);
`crates/goose-agent/src/inference.rs:184` (`InferenceRunner`), `:193` (`messages_for_provider`),
`:245` (`should_infer`); `crates/goose/src/agents/state_machine/session.rs:169` (`run_goose`).

**Operações que fecham o turno**
`ops_maxturns.rs`, `ops_retry.rs`, `ops_stop_hook.rs`, `ops_exit_on_error.rs`, `ops_steer.rs`,
`ops_toolcalling.rs:678` (`pending_advertised_tool_requests`), `:832` (execução).

**Sessão**
`crates/goose/src/session/session_manager.rs:62` (`Session`), `:316` (`SessionManager`), `:420`
(`create_session`), `:432` (`get_session`), `:455` (`add_message`), `:459`
(`replace_conversation`); `crates/goose/src/agents/types.rs:71` (`SessionConfig`).

**Conversa / tipos**
`crates/goose-provider-types/src/conversation.rs:14` (`Conversation`), `:178`
(`agent_visible_messages`), `:235` (`fix_conversation`), `:273` (`fix_messages`), `:522`
(`merge_consecutive_messages_for_request`), `:629` (`EffectiveRole`), `:645` (`effective_role`);
`conversation/message.rs` (`Message`, `MessageContentBlock`, `MessageMetadata`).

**Provider / I/O**
`crates/goose-provider-types/src/base.rs:330` (`MessageStream`), `:497` (`Provider`), `:510`
(`stream`); `crates/goose-providers/src/openai_compatible.rs:237` (`stream_openai_compat`);
`crates/goose-provider-types/src/formats/openai.rs:709` (`response_to_message`), `:1234`
(`response_to_streaming_message`), `:1664` (`create_request`).

**Eventos / CLI**
`crates/goose-agent/src/events.rs:9` (`AgentEvent`); `crates/goose-cli/src/session/mod.rs:587`
(`run_interactive`), `:780` (`handle_message_input`), `:1289` (`headless`), `:1301`
(`process_agent_response`), `:1646` (`handle_interrupted_messages`).
