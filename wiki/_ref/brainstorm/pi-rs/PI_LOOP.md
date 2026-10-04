# PI_LOOP — loop, turnos, tools e I/O do modelo (pi)

> **O que é.** Mergulho de código no **controlo do loop** do `pi`: onde começa um turno, onde acaba,
> como a sessão é recarregada e como o pedido/resposta do modelo são montados e descodificados. É o
> complemento operacional de [`05 — Runtime do agente`](05-runtime-agente.md),
> [`04 — Camada de IA`](04-camada-ai.md), [`07 — Ferramentas`](07-ferramentas.md),
> [`10 — Sessões e storage`](10-sessoes-e-storage.md) e [`11 — Protocolo e servidor`](11-protocolo-e-servidor.md):
> aqui não se repete a arquitectura, mapeia-se **peça a peça o que o loop faz com ela**.

**Alvo:** `_REF/pi/` (monorepo TypeScript `@earendil-works/*`).
**Pacotes relevantes:** `packages/agent` (loop), `packages/ai` (providers/streaming),
`packages/coding-agent` (produto), `packages/protocol` + `packages/server` (RPC), `packages/tui`.
**Método:** leitura directa do código; cada afirmação tem `ficheiro:linha` (relativo a `_REF/pi/`).

---

## 0. Sumário em uma frase

O `pi` tem **duas camadas** atrás de interfaces distintas: o **`Agent`/`agent-loop`** — um loop de
dois níveis (interno: tool calls + *steering*; externo: *follow-up*) que devolve um **stream de
eventos** (`AgentEvent`) e cujo passo é *uma chamada ao provider* — e o **`Harness`/`runtime`** —
uma máquina de estados **durável** (`driveOperation`) que persiste cada passo numa **árvore de
entradas** e retoma exactamente onde parou. Ambos operam sobre a mesma abstração de mensagem
(`AgentMessage`) e o mesmo contrato de provider (`StreamFn` → `AssistantMessageEventStream`). O que
muda é **quem decide o próximo passo**: um `while` de dois níveis com *callbacks* de decisão
(`finishTurn`), ou um *switch* sobre o estado durável da operação.

---

## 1. Vocabulário operacional (código, não analogia)

| Termo | Tipo | Onde | Definição operacional |
|---|---|---|---|
| **Mensagem do agente** | `AgentMessage` | `packages/agent/src/types.ts:370` | união de `Message` (do `pi-ai`) com `CustomAgentMessages` (declaration merging) |
| **Mensagem LLM** | `Message` | `packages/ai/src/types.ts` | `SystemMessage \| UserMessage \| AssistantMessage \| ToolResultMessage` |
| **Bloco de conteúdo** | `AssistantMessage["content"][n]` | `packages/ai/src/types.ts` | `TextContent \| ThinkingContent \| ToolCall \| ImageContent` |
| **Tool** | `AgentTool` | `packages/agent/src/types.ts:443` | `{ name, label, description, parameters (TypeBox), execute, prepareArguments?, replay?, executionMode? }` |
| **Tool call** | `AgentToolCall` | `packages/agent/src/types.ts:58` | `{ id, name, arguments }` extraído do `content` do assistente |
| **Resultado de tool** | `AgentToolResult` | `packages/agent/src/types.ts:420` | `{ content, details, usage?, terminate? }` |
| **Contexto** | `AgentContext` | `packages/agent/src/types.ts:471` | `{ messages: AgentMessage[], tools?: AgentTool[] }` |
| **Evento** | `AgentEvent` | `packages/agent/src/types.ts:485` | `agent_start/end`, `turn_start/end`, `message_start/update/end`, `tool_execution_start/update/end` |
| **Stream do provider** | `AssistantMessageEvent` | `packages/ai/src/types.ts:741` | `start`, `text_*`, `thinking_*`, `toolcall_*`, `done`, `error` |
| **Turno** | — | ver §4 | uma resposta do assistente **+ as tools que ela pede**; a unidade do `turn_start`/`turn_end` |
| **Passo do drive** | `state.at` | `packages/agent/src/harness/runtime/drive.ts:56` | um estado durável da operação (`starting`, `assistant.ready`, `tools`, …) |
| **Run** | `operationId` | `packages/agent/src/harness/session/types.ts:75` | uma operação durável (run/compaction/navigation) com resultado terminal |

**Nota de ambiguidade:** "turno" no `pi` é **uma resposta do assistente mais as suas tool calls** —
não uma mensagem do utilizador. O utilizador é um `message_start/end` **dentro** do primeiro turno
(`agent-loop.ts:105-108`), não um turno separado.

---

## 2. Entrada única e duas camadas

### 2.1 Camada baixa — `Agent` / `agent-loop`

A porta é `Agent.prompt` (`agent.ts:373`), que normaliza o input e chama `runAgentLoop`
(`agent-loop.ts:101`). O resultado é um `EventStream<AgentEvent, AgentMessage[]>` terminado por
`agent_end` (`agent-loop.ts:152-159`).

```
Agent.prompt(texto)
  └─ runWithLifecycle (agent.ts:507)   ← cria AbortController + estado isStreaming
       └─ runAgentLoop (agent-loop.ts:101)
            ├─ agent_start, turn_start, message_* (input)
            └─ runLoop (agent-loop.ts:162)
```

Há também `Agent.continue` (`agent.ts:384`) → `runAgentLoopContinue` (`agent-loop.ts:127`), que
**não** acrescenta mensagem nova: exige que a última mensagem seja `user` ou `toolResult`
(`agent-loop.ts:132-138`) e é usada por retries e continuações.

### 2.2 Camada alta — `Harness` / `runtime` (durável)

`createAgentHarness` (`harness/agent-harness.ts:614`) devolve um `AgentHarness`
(`agent-harness.ts:586`) com **lanes**. Cada lane expõe `prompt/compact/navigateTree/abort/resume`
(`harness/runtime/lane.ts:1133`, `:1200`, `:1232`, `:1373`, `:1327`) e publica `HarnessEvent`
(`agent-harness.ts:200+`). O motor é `driveOperation` (`harness/runtime/drive.ts:29`), um `switch`
sobre `operation.state.at`:

```
starting → checkpoint → assistant.ready → tools → … → summary.* → navigation.*
                        ↑ assistant.retry_wait / assistant.effect_pending
```

Cada estado é um **procedimento durável** (`drive/checkpoint.ts`, `drive/generation.ts`,
`drive/tools.ts`, `drive/reconcile.ts`, `drive/recovery.ts`, `drive/structural.ts`,
`drive/deferred.ts`). O `DriveOutcome` (`agent-harness.ts`) é `settled` ou `waiting` (retry/deferred)
— o *caller* retoma depois. **É a diferença central para o loop baixo:** o estado da operação está
**persistido** (não em memória), pelo que um crash a meio retoma com `recoverAssistantGeneration` /
`recoverStructuralGeneration`.

### 2.3 Quem usa qual

O `coding-agent` usa a camada baixa: `AgentSession` (`coding-agent/src/core/agent-session.ts:340`)
constrói um `Agent`, subscreve os eventos (`:451`) e enriquece o loop com *hooks*
(`beforeToolCall` `:590`, `afterToolCall` `:611`, `finishTurn` `:768`,
`prepareNextTurnWithContext` `:785`). O `Harness`/`runtime` é o núcleo do modo durável ("Pico"),
usado pelos modos com sessão em árvore/remota.

---

## 3. O loop (`runLoop`, `agent-loop.ts:162`)

O controlo do turno é uma **máquina de dois níveis** dentro de `runLoop`:

```
pendingMessages = getSteeringMessages()            // agente-loop.ts:170
while true:                                        // ── externo: follow-ups
  hasMoreToolCalls = true
  while hasMoreToolCalls or pendingMessages:       // ── interno: tools + steering
    if lastCompletedTurn:                          // :178
      nextTurn = prepareNextTurn(lastCompletedTurn)  // :179  (pode trocar ctx/model/thinking)
      poll steering se ainda não há                  // :193
      emit turn_start                                // :199
    para cada mensagem (prepared + pending):        // :202
      declareToolChanges(...) → message_start/end   // :203
    requestUpdate = prepareRequest(ctx, model, lvl)  // :210  (antes de TODA a requisição)
    assistant = streamAssistantResponse(...)         // :227
    if stopReason ∈ {error, aborted}:                // :229
      finishTurn; turn_end; agent_end; return        // :233-238  (hard exit)
    toolCalls = assistant.content.filter(toolCall)   // :241
    if toolCalls:
      batch = stopReason=="length"
        ? failToolCallsFromTruncatedMessage(...)     // :248  (não executa!)
        : executeToolCalls(...)                      // :252
      hasMoreToolCalls = !batch.terminate            // :255
    lastCompletedTurn = {...}                        // :259
    decision = finishTurn(lastCompletedTurn)         // :264
    emit turn_end                                    // :265
    if decision.action == "end": agent_end; return   // :267
    explicitContinuation = decision.action == "continue"  // :271
    pendingMessages = getSteeringMessages()          // :273
  followUp = getFollowUpMessages()                   // :279
  if followUp: pendingMessages = followUp; continue  // :281
  if explicitContinuation: explicitContinuation=false; continue  // :287
  break                                              // :292
agent_end                                            // :294
```

Regras operacionais (todas verificáveis no código):

- **Steering** entra **depois** do turno do assistente corrente (`:273`), ou seja, entre tool calls e
  a próxima requisição; **follow-up** só quando o agente ia parar (`:279`).
- **`prepareNextTurn`** corre após `turn_end` e pode substituir `context`, `model`, `thinkingLevel` e
  injectar mensagens (`:178-199`). É o ponto de compactação.
- **`prepareRequest`** corre **antes de cada requisição, incluindo a primeira** (`:210`), depois de
  as mensagens pendentes já terem sido anexadas e emitidas.
- **`finishTurn`** corre para respostas normais, de erro e abortadas (`:233`, `:264`); só as decisões
  de respostas **normais** são aplicadas (`{end}`/`{continue}`).
- `{ action: "continue" }` garante **uma** próxima requisição; se houver tool results, steering ou
  follow-up, esses satisfazem-na e não se acrescenta um pedido extra (`:271-277`).
- Sem mensagens novas, o loop termina (`:292`).

---

## 4. Turnos

Um **turno** é o par `turn_start` … `turn_end` (`agent-loop.ts:199`, `:265`). O `turn_end` carrega
`{ message, toolResults }` (`types.ts:489`). O `agent_start`/`agent_end` envolvem o run inteiro
(`:105`, `:294`).

Sequência canónica (do `README` do pacote e do código):

```
agent_start
turn_start
  message_start/end  { user }
  message_start      { assistant }          ← streamAssistantResponse
  message_update*    { partial }
  message_end        { assistant }
  tool_execution_start { id, name, args }
  tool_execution_update* { partial }        ← opcional (tools com streaming)
  tool_execution_end   { result, isError }
  message_start/end  { toolResult }
turn_end             { message, toolResults }
… (turn_start seguinte se houver tool calls / steering / follow-up)
agent_end            { messages }
```

Fim **anormal**: `stopReason ∈ {error, aborted}` faz `turn_end` + `agent_end` imediatos
(`:229-238`); no `Agent`, uma excepção do executor é convertida numa mensagem de falha e nos mesmos
eventos (`agent.ts:532-548`).

---

## 5. O stream do assistente (I/O do modelo)

`streamAssistantResponse` (`agent-loop.ts:380`) é a **fronteira** `AgentMessage[] → Message[]`:

1. `config.transformContext` (`:383`) — poda/injecta no nível `AgentMessage`.
2. `config.convertToLlm` (`:388`) — filtra mensagens UI-only e converte tipos custom
   (`agent.ts:29` é o `defaultConvertToLlm`, que mantém `system/user/assistant/toolResult`).
3. `normalizeContext({ messages })` (`:390`).
4. `config.getApiKey(provider)` (`:393`) — resolve tokens expiráveis a cada chamada.
5. `streamFunction(model, llmContext, options)` (`:397`) — o `StreamFn` injectável
   (`types.ts:31`; `Models.streamSimple` em `packages/ai/src/models.ts:889`).

O stream do provider é `AssistantMessageEvent` (`packages/ai/src/types.ts:741`): `start` cria a
mensagem parcial e faz `message_start` (`:344-348`); `text_*`/`thinking_*`/`toolcall_*` substituem o
último item de `context.messages` e emitem `message_update` (`:350-368`); `done`/`error` finalizam,
fazem `message_end` e devolvem a mensagem autoritativa (`:370-377`). O **`partial`** é o helper
"resposta-até-agora" partilhado, não um snapshot por evento.

Contrato do `StreamFn` (`types.ts:31-38`): **não pode lançar** por falha de request/modelo; devolve
sempre um stream; as falhas vão codificadas no stream (`error`) e numa `AssistantMessage` com
`stopReason: "error" | "aborted"` + `errorMessage`.

---

## 6. Tools

### 6.1 Despacho (`executeToolCalls`, `agent-loop.ts:507`)

- Se `config.toolExecution == "sequential"` **ou** alguma tool do batch tem
  `executionMode == "sequential"`, o batch inteiro é sequencial (`:509-515`).
- **Paralelo** (`:585`): *preflight* sequencial (`prepareToolCall` por call), depois as tools
  permitidas correm em concorrência; `tool_execution_end` sai na **ordem de conclusão**, mas as
  mensagens `toolResult` são emitidas na **ordem-fonte** do assistente (`Promise.all` sobre
  `finalizedCalls`, `:638-650`).
- **Sequencial** (`:529`): prepara, executa e finaliza uma a uma; a ordem é a do assistente.

### 6.2 As três fases de cada call

1. **`prepareToolCall`** (`:705`): resolve a tool no `context.tools`; aplica
   `tool.prepareArguments` (shim de compatibilidade) e `validateToolArguments`; corre
   `config.beforeToolCall` (`:725`). Se o hook devolver `{ block: true }`, o resultado é um erro
   sintético (`:735-745`); `terminate` do block propaga-se (`:741`).
2. **`executePreparedToolCall`** (`:775`): chama `tool.execute(id, args, signal, onUpdate)`. Cada
   `onUpdate` vira `tool_execution_update` (`:783-793`). Excepções tornam-se resultado de erro
   (`:806-810`).
3. **`finalizeExecutedToolCall`** (`:818`): corre `config.afterToolCall` (`:823`), que pode
   substituir `content`, `details`, `usage`, `terminate`, `isError` campo a campo (`types.ts:88-113`).

### 6.3 Regras de lote

- **`terminate`** (`:687`): o batch só termina cedo se **todos** os resultados finalizados tiverem
  `terminate === true`; batches mistos continuam.
- **Truncagem por tokens** (`:477`): se `stopReason == "length"`, **nenhuma** tool call do batch é
  executada — todas viram erro a dizer para reformular (`:485-505`). Isto é a defesa contra
  argumentos JSON "salvos" mas incompletos.
- **Ferramenta em falta / args inválidos**: `prepareToolCall` devolve `ImmediateToolCallOutcome` com
  erro; nunca lança (`:710-717`, `:770-773`).

### 6.4 As tools do produto

`packages/coding-agent/src/core/tools/index.ts` define o catálogo: `read`, `bash`, `powershell`,
`edit`, `write`, `grep`, `find`, `ls` (`ToolName`, `allToolNames`). Há três recortes:
`createCodingTools` (read/bash/edit/write), `createReadOnlyTools` (read/grep/find/ls) e
`createAllTools`. Cada tool é criada em dois sabores: `createXTool` (`AgentTool`, executa) e
`createXToolDefinition` (`ToolDefinition`, para extensões/render). O `bash` (`core/tools/bash.ts`)
mostra o padrão: schema TypeBox, `execute` que corre `env.exec` com `capture` (limites de bytes/linhas,
spill para ficheiro), `onUpdate` com *checkpoint* a cada 2 s, e erro como `throw` (o loop converte-o).

No `harness` há um segundo conjunto (`harness/tools/`: `read`, `write`, `edit`, `bash`, `image`) com
contrato `AgentHarnessTool` (recebe `toolContext`, `invocation`, `context`). A execução é decomposta
em funções puras em `harness/execution/tools.ts` (`prepareToolCall`, `applyBeforeToolDecision`,
`executeToolCall`, `finalizeToolCall`, `createToolResultMessage`), e o `drive/tools.ts` faz a
publicação durável do *intent* antes do efeito.

---

## 7. Eventos

### 7.1 `AgentEvent` (camada baixa, `types.ts:485`)

`agent_start` · `agent_end{messages}` · `turn_start` · `turn_end{message, toolResults}` ·
`message_start{message}` · `message_update{message, assistantMessageEvent}` · `message_end{message}` ·
`tool_execution_start{id,name,args}` · `tool_execution_update{partialResult}` ·
`tool_execution_end{result,isError}`.

- Só `message_update` é emitido **apenas** para o assistente em streaming (`types.ts:491`).
- `Agent.subscribe` (`agent.ts:266`) registra *listeners* **aguardados em ordem de subscrição**
  (`agent.ts:565-579`); `agent_end` é o último evento, mas `waitForIdle` só resolve depois de os
  listeners de `agent_end` terminarem (`agent.ts:350`).
- O estado do `Agent` é reduzido **antes** de notificar (`processEvents`, `agent.ts:565`):
  `message_end` faz `push` à transcrição (`:571`), `tool_execution_start/end` mantêm
  `pendingToolCalls` (`:575-586`), `turn_end` guarda `errorMessage` (`:588`).

### 7.2 `HarnessEvent` (camada alta, `agent-harness.ts:200+`)

Mais rico: `run_start/resume/suspend/end`, `operation_abort`, `turn_start/end{runId,turnId,…}`,
`retry_scheduled/start/end`, `message_*`, `tool_start/update/end`, `entry_added`, `queue_update`,
`config_update`, `compaction_*`, `navigation_*`, `lane_created`, `usage`, `fault`, `handler_error`.
O barramento (`harness/events.ts:6`) entrega por **tipo** e por **watcher** (`watch`), isola falhas
(um listener que lança vira `handler_error`) e serializa a entrega num `deliveryTail`.

---

## 8. Sessão e durabilidade

- A sessão é uma **árvore de entradas** (`Entry`: `MessageEntry | CompactionEntry | BranchSummaryEntry
  | CustomEntry`, `harness/session/types.ts:64`), com `parentId` e `seq` (`:18-25`). É append-only por
  branch; trocar de branch resume o caminho abandonado num `BranchSummaryEntry` (`:43`).
- Cada run é uma `OperationMeta` (`:75`) com `intent` (`run`/`compaction`/`navigation`) e `Control`
  (`running`/`cancel_requested`, `:92`); o terminal é `OperationResultRecord` (`:108`).
- A `Continuation` (`:119`) modela o que falta (`need_assistant`, overflow recovery) — é o que o
  `drive` consulta para decidir o próximo `state.at`.
- O storage concreto é JSONL em árvore (`harness/session/jsonl/`): `repo.ts` (`JsonlSessionRepo`),
  `codec.ts`, `io.ts`, `fork.ts`, `legacy-v3.ts`. Há também backends SQLite em pacote separado
  (`packages/session-backends`).
- O `coding-agent` mantém a sua própria projecção (`SessionManager`, `agent-session.ts:342`):
  `appendMessage` (`:1029`), `buildSessionProjection` (`:658`), `appendModelChange` (`:573`),
  `appendContextEdit` — a transcrição que o modelo vê é **derivada** da árvore.

---

## 9. Protocolo e superfícies

O `pi` tem **três** formas de um front-end falar com o agente: (a) **in-process** (o caso normal
— o `AgentSession` corre no processo do CLI e a TUI consome `AgentSessionEvent` directamente);
(b) um **RPC binário cliente/servidor** (CBOR + framing) que hospeda a sessão num processo
servidor e a liga a um ou mais clientes; (c) um **modo `rpc` JSONL** (stdin/stdout) para
*embedding*, distinto do anterior. Só (b) é "o protocolo" abaixo.

### 9.1 O que é

Um **RPC de serviço, binário e enquadrado, sobre um stream de bytes** (Unix domain socket ou
stdio). O protocolo **não** conhece o agente: o campo `call` é uma chamada de serviço **opaca**
(`{serviceId, member, args}`), descodificada pelo `chord` (`parseServiceCall`). O agente é apenas
**um** serviço; a mesma moldura serve facetas/extensões (`chord/src/services/`).

### 9.2 Framing e codec

- **Framing** (`packages/protocol/src/framing.ts`): cada payload é prefixado por **4 bytes
  big-endian** com o comprimento (`FRAME_HEADER_LENGTH = 4`), teto por omissão de **16 MiB**
  (`DEFAULT_MAX_FRAME_LENGTH = 16 * 1024 * 1024`). O `FrameDecoder` é **incremental** (aceita
  chunks arbitrários, junta blocos de 64 KiB) e rejeita frames truncados no `end()`.
- **Codec** (`packages/protocol/src/codec.ts`): `encode*Message` valida com TypeBox (`Check`) +
  `isJsonValue`, codifica em **CBOR** com `maxByteLength` e enquadra; `*MessageDecoder` faz o
  inverso (frame → CBOR → valida). Um erro de framing/validação é **fatal** para a ligação.
- **CBOR** (`packages/protocol/src/cbor/`): binário compacto, mais pequeno que JSON e sem
  ambiguidade de números; o `maxByteLength` é o mesmo teto do frame.

### 9.3 Esquema das mensagens (`packages/protocol/src/protocol.ts`, `PROTOCOL_VERSION = 8`)

Objetos **estritos** (`additionalProperties: false`); `IdSchema` = string não vazia; `ServerId` =
UUIDv4 minúsculo canónico.

| Direcção | Mensagem | Campos |
|---|---|---|
| cliente → servidor | `hello` (`:29`) | `{type, version}` — **tem** de ser a primeira |
| cliente → servidor | `request` (`:49`) | `{type, id, target, call}` |
| cliente → servidor | `cancel` (`:55`) | `{type, id, target}` |
| servidor → cliente | `hello` (`:65`) | `{type, version: 8, serverId}` |
| servidor → cliente | `hello_error` (`:70`) | `{type, error:{code,message}}` |
| servidor → cliente | `response` (`:74`) | `{type, id, ok:true, result?}` ou `{ok:false, error}` |
| servidor → cliente | `service_update` (`:88`) | `{type, subscriptionId, update}` |
| servidor → cliente | `attachment` (`:94`) | `{type, attachment: SessionTarget \| null}` |

O **`target`** (`:41-46`) é `{serverId}` (chamada ao servidor) ou
`{serverId, sessionId, attachmentId}` (chamada à sessão). O `attachmentId` é o **lease** que o
cliente detém sobre a sessão: uma chamada só é aceite se `target` casar com o lease actual
(`session-router.ts::requireAttachment`).

### 9.4 Handshake

1. O cliente abre o transporte e envia `hello{version}` (`client/src/connection.ts`).
2. O servidor, em `stage: "awaitingHello"`, recusa qualquer primeira mensagem que não seja
   `hello` (`server.ts::dispatchMessage`); verifica `isSupportedProtocolVersion` e responde
   `hello{version: 8, serverId}` (`finishHandshake`). Há um **timeout de handshake** de 5 s
   (`DEFAULT_HANDSHAKE_TIMEOUT_MS`); uma versão errada fecha com `hello_error`.
3. O cliente valida que o `serverId` é o esperado e só então passa a `connected`; um erro antes
   disso rejeita o *future* do handshake.

### 9.5 Pedido, cancelamento e correlação

- O cliente gera `id = "request-N"`, guarda-o num `Map` de pendentes e envia `request`. A
  `response` com o mesmo `id` resolve/rejeita o pendente (`client.ts::#request`/`#handleMessage`).
- **Cancelamento**: um `AbortSignal` do cliente envia `cancel{id,target}`; o servidor aborta o
  `AbortController` daquele pedido (`server.ts::handleCancel`) e a `response` final vem com
  `{code: "cancelled"}`. O servidor rejeita um `id` já activo.
- **Subscrições/streaming**: uma `call` de `subscribe` devolve um **snapshot** e o servidor
  passa a emitir `service_update` com **deltas codificados por estado**
  (`createServiceStateEncoder`), não o estado inteiro; `unsubscribe` remove o encoder. Isto é o
  que dá streaming contínuo (eventos do agente) sem repetir o snapshot.

### 9.6 Servidor e cliente

- **Servidor** (`packages/server/src/server.ts`): aceita ligações (`listener`), mantém por ligação
  um `ConnectionState` (decoder, `stage`, `activeRequests`, encoders de subscrição), e roteia.
  `SessionRouter` (`session-router.ts`) **hospeda** sessões (`HostedSession`), **liga** clientes
  (`ClientAttachment` com `attachmentId`, conjunto de operações e lease) e **serializa** as
  operações de cada cliente (`runForClient`). `openSession`→`attachClient`→`invokeService`.
- **Cliente** (`packages/client/src/client.ts`): `Client.connect()` → `Connection` (handshake) →
  `request(target, call, signal)`; `onAttachmentChange`/`onConnectionStateChange`;
  `subscribeService` (snapshot + updates, com *backpressure* por `deliveryTail`).
- **Transporte** (`packages/client/src/unix.ts`): **Unix domain socket**; `discoverUnixServers`
  descobre sockets `<serverId>.sock` e faz *probe* de handshake; o servidor tem
  `server/src/transports/unix/`.

### 9.7 Onde é usado (e onde não é)

O RPC CBOR é usado apenas no **topologia remota/servidor** do `coding-agent`:
`src/experimental/server.ts`, `client-runtime.ts`, `radius-relay.ts`,
`session-worker-manager.ts`. O **modo interactivo normal corre in-process** (o `AgentSession` vive
no processo do CLI). O **modo `rpc`** (`src/modes/rpc/rpc-mode.ts`) é um **protocolo diferente** —
JSONL por stdin/stdout com `RpcCommand`/`RpcResponse`/eventos — para *embedding*.

### 9.8 O ponto para o `katu`

Este protocolo é um **limite de processo** (cliente↔servidor) e existe no `pi` porque o produto
tem **vários front-ends** (TUI, print, rpc, partilha de sessão, *handoff* remoto) e sessões
**hospedadas** que vários clientes ligam com *lease*. O `katu` decidiu **não** ter essa fronteira
(ADR 0027): um front-end, um kernel de uma thread, `Command`/`Event` in-process. A comparação e o
porquê de não adoptar uma "versão melhorada" estão em
[`PI_VS_KATU_LOOP.md`](PI_VS_KATU_LOOP.md) §3 (D7).

---

## 10. Providers

`packages/ai/src/models.ts:889` — `Models.streamSimple(model, context, options)` despacha para o
provider do modelo. Cada API (`api/openai-completions.ts`, `api/anthropic-messages.ts`,
`api/google-generative-ai.ts`, `api/openai-responses.ts`, …) produz o mesmo
`AssistantMessageEventStream`. `SimpleStreamOptions` carrega `sessionId` (cache-aware), `transport`,
`thinkingBudgets`, `onPayload`/`onResponse`/`onProviderStreamEvent`, `maxRetryDelayMs`.

O `EventStream` (`packages/ai/src/utils/event-stream.ts`) é a primitiva: um stream de eventos com
`push`/`end` e um `result()` que resolve a mensagem final — é o que o `agent-loop` consome e o que o
`Agent` expõe ao exterior.

---

## 11. Decisões e lições (para a comparação)

1. **Um passo = uma chamada ao provider.** O loop não conhece "estados"; conhece um stream e um
   `stopReason`. A durabilidade vive numa camada **acima** (`drive`), não no loop.
2. **A decisão de fim é um *callback* (`finishTurn`), não um `if`.** Isto permite ao produto
   (`coding-agent`) decidir continuar/terminar sem tocar no loop.
3. **Truncagem é um erro *ao modelo*, não do turno.** `stopReason == "length"` fecha todas as tool
   calls do batch como erro e o modelo reformula (`:477`).
4. **`terminate` é do lote inteiro.** Uma tool "terminal" só corta se todas as do batch o forem.
5. **Steering e follow-up são filas separadas** com modo `all`/`one-at-a-time` (`types.ts:55`),
   drenadas em pontos distintos do loop.
6. **O stream é a interface.** A UI consome `AgentEvent`; nada bloqueia a thread da UI.
7. **Sessão = árvore append-only de entradas**, com compaction/branch summary como entradas de
   primeira classe — o histórico é reconstruível e ramificável.
8. **O protocolo RPC é opcional e acima da camada baixa.** O `Agent` não depende dele.

A comparação com o `katu` (o que incorporar, o que evitar) está em
[`PI_VS_KATU_LOOP.md`](PI_VS_KATU_LOOP.md).

---

## 12. Referências

**pi:** `packages/agent/src/agent-loop.ts` (`:101` `runAgentLoop`, `:162` `runLoop`, `:380`
`streamAssistantResponse`, `:507` `executeToolCalls`, `:705` `prepareToolCall`),
`packages/agent/src/agent.ts` (`:188` `Agent`, `:373` `prompt`, `:507` `runWithLifecycle`, `:565`
`processEvents`), `packages/agent/src/types.ts` (`:189` `AgentLoopConfig`, `:443` `AgentTool`, `:485`
`AgentEvent`), `packages/agent/src/harness/runtime/drive.ts` (`:29` `driveOperation`),
`packages/agent/src/harness/execution/tools.ts`, `packages/agent/src/harness/session/types.ts`,
`packages/protocol/src/protocol.ts`, `packages/protocol/src/framing.ts`,
`packages/coding-agent/src/core/agent-session.ts` (`:340` `AgentSession`, `:1708` `prompt`),
`packages/coding-agent/src/core/tools/index.ts`.

**Dossiê:** [`README`](README.md) · [`00 — Filosofia`](00-filosofia-do-pi.md) ·
[`05 — Runtime do agente`](05-runtime-agente.md) · [`07 — Ferramentas`](07-ferramentas.md) ·
[`10 — Sessões e storage`](10-sessoes-e-storage.md) · [`11 — Protocolo e servidor`](11-protocolo-e-servidor.md).
