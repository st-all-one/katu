# PI_VS_KATU_LOOP — o que incorporar no loop do katu (a partir do pi)

> **O que é.** Comparação directa entre o controlo de loop do `pi` (mapeado em
> [`PI_LOOP`](PI_LOOP.md)) e o do `katu`, com as diferenças provadas por código e o que
> incorporar, por prioridade. Não repete a arquitectura geral (ver
> [`15 — Riscos e decisões`](15-riscos-e-decisoes.md)); foca o **controlo do turno**.

**Alvo katu:** `crates/katu/src/agent/`, `crates/katu-core/src/kernel/`, `crates/katu-core/src/api/`.
**Alvo pi:** `_REF/pi/` · `packages/agent`, `packages/ai`, `packages/coding-agent`.
**Método:** cada afirmação tem `ficheiro:linha`.

---

## 1. O que o katu já faz bem (não mexer)

| Mecanismo | Onde | Porque é melhor que o pi |
|---|---|---|
| **Log append-only como fonte da verdade** | `kernel/session/mod.rs`, `kernel/log.rs` | Replay byte-a-byte e `Model-visible ⟺ logged`; no pi os `AgentEvent` são efémeros e a árvore de sessão é uma projecção separada |
| **Ordem §42 imposta** | `kernel/session/mod.rs::tool_call` | `ToolCall` logado **antes** do efeito; no pi a tool corre e só depois vira mensagem |
| **Política pura e fail-closed** | `kernel/pipeline/mod.rs::dispatch_with` | A tool não é invocada num `Deny`; o pi tem só um `beforeToolCall` opcional sem política |
| **Aprovação com MAC e one-shot** | `agent/turn.rs::retry_with_approval`, `session.approve/revoke_approval` | Mais forte que o `block` do pi (sem criptografia, sem capacidade mínima) |
| **Guard de loop (CUSUM + e-value)** | `kernel/guard.rs`, `agent/turn/run.rs::cut_if_looping` | O pi **não** tem guard no loop |
| **Deteção de eco do delta de tool** | `agent/turn/echo.rs`, `run.rs:164` | O pi não tem equivalente (rede contra TOON/JSON cru) |
| **Resposta vazia tratada no loop** | `run.rs:164` (`MAX_EMPTY_RETRIES`) + `Termination::Empty` | O loop do pi não distingue vazio; só o `coding-agent` faz retry |
| **Tool calls declaradas em texto** | `agent/turn/declared.rs` | Degrau para modelos locais sem tool calls nativas; o pi depende do provider |
| **Lock de turno por sessão** | `kernel/session/lock.rs` (L-Q6) | O pi não serializa dois processos sobre a mesma sessão |
| **Reconciliação de pendentes** | `kernel/session/reconcile.rs` (L-Q1) | Fecha `ToolCall` sem `ToolResult` com `Unavailable{interrupted}` |
| **Truncagem por call, no log** | `agent/failure.rs::settle_truncated` | O pi fecha o batch inteiro; o katu fecha cada `CallId` com delta que ensina |

O objectivo **não** é trocar isto. É acrescentar os degraus que faltam.

---

## 2. Comparação peça a peça

| Dimensão | katu (hoje) | pi | Veredicto |
|---|---|---|---|
| Unidade do loop | `drive` (passo = `provider.stream`) | `runLoop` de **dois níveis** (interno: tools+steering; externo: follow-up) | katu OK (mais simples); pi separa melhor as filas |
| Decisão de fim de turno | `Termination` + `finish` (enum fixo) | *callback* `finishTurn → {end\|continue}` | **pi flexível**; katu não deixa a superfície decidir continuar |
| Steering | `ActivitySink::steer()` sondado **depois** de `run_calls` | fila drenada após o turno (`:273`) e no início (`:170`), modo `all`/`one-at-a-time` | katu OK; pi tem modo de drenagem |
| Follow-up | um `Submit` fica **bufferizado** no canal e corre após `Done` | fila explícita `getFollowUpMessages`, drenada quando o agente ia parar | paridade estrutural (katu não precisa de fila) |
| Troca de modelo/thinking a meio do run | só por comando (`SetModel`/`SetThinking`) → **próximo** turno | `prepareNextTurn` troca contexto/modelo/thinking **dentro** do run | **pi flexível** (compactação/routing mid-run) |
| `prepareRequest` antes de cada pedido | `build_request` reconstrói do log (sem hook) | *callback* que pode substituir contexto/modelo antes de **cada** pedido | katu OK (log é a fonte) |
| Execução paralela | lotes de `Shared` consecutivos; `exclusive` = barreira; `MAX_PARALLEL_CALLS` | `parallel` por omissão; `executionMode` por tool; batch vira sequencial se alguma o pedir | paridade |
| Ordem dos resultados | commit na ordem-fonte | `tool_execution_end` na ordem de conclusão; mensagens na ordem-fonte | paridade |
| Truncagem (`length`) | `settle_truncated` → `Unavailable{length}` por call | `failToolCallsFromTruncatedMessage` → erro por call, **não** executa | paridade (katu loga por call) |
| Resposta vazia | retry + `Termination::Empty` visível | loop não trata; `coding-agent` faz retry | katu melhor |
| Eco de tool | `echoes_recent_delta` + nota visível | — | katu melhor |
| Guard de loop | `Guard` (CUSUM/e-value) | — | katu melhor |
| Hooks de tool | política pura + aprovação MAC | `beforeToolCall` (block/args) / `afterToolCall` (override) | katu melhor (política); pi permite **reescrever** o resultado |
| `terminate` do lote | — | `terminate: true` em **todas** as calls corta o batch | **pi tem; katu não** |
| Streaming de tools | `Activity::Tool` (args) + `ToolDone` (resumo) | `tool_execution_update` com resultado parcial | **pi tem; katu não** (só o resumo no fim) |
| Eventos vs log | log = fonte; `Live` efémero | `AgentEvent` efémero; sessão separada | katu mais forte |
| Sessão | log linear append-only + trash/restore + compactação | árvore de entradas (branching) + `branch_summary` | **pi tem branching**; katu é linear |
| Durabilidade/retoma | replay do log + `verify` + `reconcile_pending` | `driveOperation` durável + `effect_pending`/`recoverGeneration` + `deferred` | **pi mais fino** (retoma a geração a meio) |
| Protocolo/superfície | `Command`/`Event` **in-process** (ADR 0027) | CBOR + framing de 4 bytes + RPC cliente/servidor (`PROTOCOL_VERSION = 8`); mas o modo interactivo normal também é in-process | diferença deliberada (ver D7) |
| Lock de turno | `turn.lock` por sessão (L-Q6) | — | katu melhor |

---

## 3. As diferenças, com evidência

### D1 — O fim de turno é um `enum`, não uma decisão da superfície

- katu: `run_turn_with` devolve `TurnReport` com `termination: Termination` (`agent/mod.rs:114`) e o
  `finish` decide a nota (`run.rs:356`). A superfície não pode pedir "continua uma vez mais" sem um
  novo `Submit`.
- pi: `finishTurn` (`agent-loop.ts:264`) devolve `{action: "end"}` ou `{action: "continue"}`; o
  `coding-agent` instala-o (`agent-session.ts:768`) e pode forçar **uma** requisição extra
  (`:271`). É como a compactação e o "continue" conversacional funcionam sem novo prompt.

**Impacto:** no katu, um turno que precise de "mais uma volta" (ex.: depois de compactar) tem de
fechar e reabrir; no pi é uma decisão *inline*.

### D2 — Sem troca de modelo/thinking a meio do run

- katu: `TurnOptions.model` é fixado no `submit` (`kernel.rs:256-273`); `SetModel`/`SetThinking` são
  comandos separados (`api/command.rs:44-46`) que só afetam o turno seguinte.
- pi: `prepareNextTurn` (`agent-loop.ts:179`) devolve `model`/`thinkingLevel` que substituem a
  configuração para o turno seguinte **dentro do mesmo run**; `prepareRequest` (`:210`) faz o mesmo
  antes de cada pedido.

**Impacto:** roteamento por fase/custo a meio de um turno (ex.: "planear com um modelo, executar com
outro") não é expressável no katu sem dois `Submit`.

### D3 — `terminate`: uma tool pode encerrar o turno

- pi: `AgentToolResult.terminate` (`types.ts:420`); o batch só termina cedo se **todas** as calls
  finalizadas tiverem `terminate` (`agent-loop.ts:687`). Uma tool "concluir" pode, assim, parar o
  loop.
- katu: `run_calls` executa todas as calls do passo; não há sinal de "parar aqui" vindo da tool
  (`agent/turn.rs:139`).

**Impacto:** um verbo terminal (ex.: `finish`/`submit_plan`) não consegue cortar o passo.

### D4 — Sem streaming de output das tools

- pi: `onUpdate` da tool vira `tool_execution_update` (`agent-loop.ts:783-793`); o `bash`
  (`harness/tools/bash.ts`) emite *checkpoints* a cada 2 s. A UI vê o output a crescer.
- katu: `Activity::Tool` mostra os **argumentos** e `Activity::ToolDone` um **resumo** de uma linha
  (`agent/turn.rs:36-52`, `LIVE_FLOW` LF4). Não há `Activity` de output parcial.

**Impacto:** um `bash` longo parece parado (só o resumo no fim), apesar de o `drain_stream` já
receber a saída.

### D5 — Sessão linear vs árvore

- katu: log append-only linear; `trash`/`restore` recuperam entradas, mas não há *branching*
  (`kernel/session/`, `kernel/trash.rs`).
- pi: `Entry` tem `parentId` e `seq` (`harness/session/types.ts:18-25`); trocar de branch gera um
  `BranchSummaryEntry` (`:43`). O `coding-agent` navega a árvore (`navigateTree`,
  `harness/runtime/lane.ts:1232`).

**Impacto:** experimentar dois caminhos a partir do mesmo ponto não é possível no katu.

### D6 — Durabilidade mais fina no pi

- pi: `driveOperation` (`harness/runtime/drive.ts:29`) tem estados `assistant.effect_pending` e
  `deferred.*`; `recoverAssistantGeneration` (`drive/recovery.ts`) retoma uma geração cujo *intent*
  foi publicado mas cujo resultado não chegou.
- katu: o log é a fonte; uma geração interrompida simplesmente **não** tem `assistant_message`, e a
  retomada reconcilia as tool calls pendentes (`kernel/session/reconcile.rs`), mas não retoma a
  chamada ao provider.

**Impacto:** o pi recupera trabalho de geração em voo; o katu recomeça o passo. Para o modelo
in-process do katu, é aceitável (a chamada não é durável por natureza).

---

### D7 — Protocolo: RPC de serviço vs canal in-process

**O que o pi tem** (detalhado em [`PI_LOOP`](PI_LOOP.md) §9): um **RPC de serviço, binário e
enquadrado**, sobre Unix socket/stdio — 4 bytes de comprimento big-endian + payload **CBOR**
(`protocol/src/framing.ts`, `codec.ts`), mensagens `hello`/`request{id,target,call}`/
`cancel{id,target}`/`response`/`service_update`/`attachment` (`protocol.ts`, `PROTOCOL_VERSION = 8`),
handshake com timeout, `target` com **lease** (`attachmentId`), servidor que **hospeda** sessões e
liga clientes (`session-router.ts`). O `call` é **opaco** — um serviço genérico do `chord`; o
agente é apenas um serviço.

**Para que serve.** Não é o caminho normal: o modo interactivo corre **in-process** (o
`AgentSession` no processo do CLI). O RPC só é usado na topologia **remota/servidor**
(`coding-agent/src/experimental/{server,client-runtime,radius-relay,session-worker-manager}.ts`),
para **partilha de sessão**, *handoff* entre dispositivos e múltiplos clientes ligados à mesma
sessão com *lease*. O modo `rpc` do CLI é ainda **outro** protocolo (JSONL stdin/stdout).

**Porque o katu não segue** (ADR 0027). O katu tem **um** front-end (TUI) e um CLI sequencial; o
kernel é **uma thread** com um único dono do `Runtime`/`Session` e **um** escritor do log (K1/K6).
Adoptar a fronteira de processo pagaria: IPC + versionamento de protocolo + serialização do estado
do kernel + um modo de falha novo (cliente desligado a meio), sem **nenhum** consumidor que o
justifique — e arriscaria a invariante "um escritor do log". A decisão é **condicionada por
evidência**: reabre-se se aparecer um **segundo front-end real** (outro processo, isolamento de
falhas ou cliente remoto) que não possa viver na thread do kernel.

**Haveria uma "versão melhorada" a seguir?** O que de bom o pi traz já está, no essencial,
preservado no katu, sem pagar o custo:

| Ideia do pi | Equivalente katu (hoje) | Vale adoptar agora? |
|---|---|---|
| Protocolo `serde`-pronto (tipos serializáveis) | `katu-core/src/api/{command,event}.rs` já são `serde` | **já existe** — a opção não se perde |
| Seam isolado superfície↔kernel | `KernelHandle`/`KernelBus` (`api/handle.rs`) é a única porta | **já existe** (K1–K8) |
| Framing binário + versão | — | só se houver transporte; então *length-prefixed* (postcard/CBOR) é a forma certa |
| Correlação `id` + `cancel` | `Command::Cancel` + `Flag` partilhada; resposta síncrona | **já existe** no in-process |
| *Lease* de attachment / múltiplos clientes | — | sem segundo cliente, é capacidade sem consumidor |
| `service_update` (deltas) | `Event`/`Live` no canal | **já existe** (LIVE_FLOW) |

Conclusão: seguir o pi **literalmente** seria uma solução à procura de problema. A "versão
melhorada" que interessa ao katu é **manter o protocolo serializável e o seam isolado** — o que
já é verdade — para que, **se** a condição de revisita do ADR 0027 se cumprir, se acrescente o
transporte sem tocar no kernel. Adoptá-lo agora violaria "nenhuma superfície sem PR que sobe o
tecto" e não tem ganho medido.

---

## 4. Candidatos a incorporar (priorizado)

> Prioridade pela hierarquia do projecto: **Q** (qualidade da execução) > **P** (performance) >
> **S** (simplificação). Nada aqui é obrigatório; cada item tem de caber no kernel de uma só thread
> e no log append-only.

### Q1 — `terminate` por resultado de tool

**Sintoma:** um verbo terminal não consegue cortar o turno.

**Mudança katu:** acrescentar `terminate: bool` ao `CallOutcome`/`ToolOutcome` (ou um campo no
`ToolReport`) e, em `run_calls`, cortar o passo quando **todas** as calls do lote o pedirem —
espelhando `shouldTerminateToolBatch` (`agent-loop.ts:687`). O fecho do turno continua a passar por
`record_turn_end`; a terminação nova entra em `Termination` (ex.: `Terminal`).

### Q2 — Troca de modelo/thinking a meio do run

**Sintoma:** "planear com um modelo, executar com outro" exige dois `Submit`.

**Mudança katu:** permitir que o **guard/roteador** (não a superfície) escolha o modelo por passo.
Já existe `TierPolicy::model_for` (`kernel.rs:258`) e `build_request` por passo
(`agent/turn/run.rs:117`). O que falta é decidir a **fase** a meio do turno e re-`build_request` com
outro modelo — sem abrir um segundo `Submit`. Um `Termination`/`Step` que carregue o modelo do passo
seguinte é suficiente.

### P1 — Streaming de output das tools

**Sintoma:** `bash` longo parece parado.

**Mudança katu:** acrescentar `Activity::ToolOutput { name, chunk }` e emiti-lo do `drain_stream` da
tool (as portas já entregam a saída). A TUI/CLI mostram as últimas N linhas (como o `LIVE_FLOW` já
faz para o texto). Mantém-se **efémero** (V4): nunca entra no log nem no contexto.

### S1 — `finishTurn` como decisão da superfície

**Sintoma:** a superfície não pode pedir "continua uma vez mais" (ex.: após compactar).

**Mudança katu:** expor no protocolo (`api/`) um `Event::TurnEnded { can_continue }` e um
`Command::Continue`; o kernel, no fim do `drive`, consulta uma flag (como o `steering`) em vez de
fechar logo. É uma mudança de contrato — avaliar antes de adoptar; o `Termination` actual cobre a
maioria dos casos.

### S2 — Árvore de sessão (branching)

**Sintoma:** não se experimentam dois caminhos a partir do mesmo ponto.

**Mudança katu:** grande. O log linear teria de ganhar `parent`/`branch` e um `BranchSummaryEntry`.
Fica fora do âmbito actual (o `trash`/`restore` cobre o "voltar atrás" simples); registar como
decisão consciente, não como lacuna.

---

## 5. Mapa sintoma → mecanismo pi → mudança katu

| Sintoma do utilizador | Mecanismo pi | Mudança katu |
|---|---|---|
| "o `bash` longo parece travado" | `tool_execution_update` + checkpoint | `Activity::ToolOutput` (P1) |
| "queria planear com um modelo e executar com outro" | `prepareNextTurn` troca `model`/`thinking` | modelo por passo no `drive` (Q2) |
| "uma tool de 'concluir' não corta o turno" | `terminate` no lote | `terminate` em `CallOutcome` (Q1) |
| "não consigo continuar após compactar sem novo prompt" | `finishTurn → continue` | `Command::Continue` (S1) |
| "não consigo ramificar a conversa" | árvore de `Entry` + `branch_summary` | fora do âmbito (S2) |

---

## 6. Plano de verificação

- **Q1:** teste de um lote onde todas as calls pedem `terminate` → o passo corta e o `turn_end` é
  logado; um lote misto **continua** (espelha `shouldTerminateToolBatch`).
- **Q2:** teste de um turno que muda de modelo entre passos → o log mostra o `estado` do passo
  seguinte com o modelo novo e `Model-visible ⟺ logged` mantém-se.
- **P1:** teste de que os chunks de output de uma tool são emitidos como `Activity` efémera e **não**
  aparecem no log nem no contexto (reutilizar o teste de `LIVE_FLOW`).
- **Invariantes:** manter `cargo xtask check` (replay, `Model-visible ⟺ logged`, guard, política,
  lock, reconciliação).
- **Regressão:** o `run_scoped` do CLI (canal fechado dentro do escopo) e o `Kernel::run` (uma
  thread) não podem ser quebrados por nenhuma destas mudanças.

---

## 7. O que **não** copiar do pi

- **`AgentMessage` extensível por declaration merging** (`types.ts:370`): em Rust, enum fechado com
  `Custom { custom_type, … }` é suficiente e mais seguro.
- **Hooks arbitrários (`beforeToolCall`/`afterToolCall`) que reescrevem o resultado**: no katu a
  política é pura e fail-closed; deixar JS arbitrário reescrever o `ToolResult` quebraria a ordem §42
  e o `Model-visible ⟺ logged`.
- **Protocolo RPC + servidor + CBOR**: o katu decidiu não ter limite de processo (ADR 0027). O
  `Command`/`Event` in-process é a fronteira. Manter os tipos `serde`-prontos é a opção barata de
  futuro; o *lease* de attachment e o serviço genérico do `chord` não têm consumidor no katu
  (ver §3 D7).
- **Máquina de estados durável de 2 000 linhas (`lane.ts`)**: complexidade justificada por sessões
  remotas/retomáveis; o katu tem um caminho só e o log como fonte.
- **Duas camadas (`Agent` e `Harness`) com semânticas sobrepostas**: o katu deve manter **um** loop.
- **Retry/erro como mensagem de assistente sintética** (`agent.ts:532`): o katu já tem
  `Event::Failure{kind,message}` e `Termination` — mais explícito.

---

## 8. Referências

**pi:** ver [`PI_LOOP`](PI_LOOP.md) §3 (loop), §4 (turnos), §6 (tools), §7 (eventos), §8 (sessão).

**katu:** `agent/turn/run.rs` (`:88` `drive`, `:164` vazio/eco, `:256` `stream_step`, `:356`
`finish`), `agent/turn.rs` (`:139` `run_calls`, `:242` `retry_with_approval`), `agent/failure.rs`
(`:60` `settle_truncated`), `agent/mod.rs:114` (`Termination`), `kernel.rs` (`:67` `run`, `:247`
`submit`), `kernel/session/reconcile.rs` (L-Q1), `kernel/session/lock.rs` (L-Q6),
`katu-core/src/api/{command.rs,event.rs}` (protocolo in-process).

**Planos relacionados:** [`KERNEL_SURFACE`](../../plan/KERNEL_SURFACE.md) (kernel de uma thread,
superfícies como clientes), [`LIVE_FLOW`](../../plan/LIVE_FLOW.md) (fluxo efémero),
[`LOOP_RESILIENCE`](../../plan/LOOP_RESILIENCE.md) (invariantes I1–I8, itens L-Q/L-P/L-S).
