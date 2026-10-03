# GOOSE_VS_KATU_LOOP — o que incorporar no loop do katu

> **O que é.** Comparação directa entre o controlo de loop do goose (mapeado em
> [`GOOSE_LOOP`](GOOSE_LOOP.md)) e o do `katu`, com as lacunas provadas por código e o que
> incorporar, por prioridade, para o loop ficar **mais amigável, resiliente, funcional e
> interrompível**. Não repete a arquitectura geral (ver [`15 — Decisões e lições`](15-decisoes-e-licoes.md));
> foca o **controlo do turno**.

**Alvo katu:** `crates/katu/src/agent/`, `crates/katu-core/src/kernel/`, `crates/katu-tui/src/`.
**Alvo goose:** `_REF/goose/` · commit `4dea9b4`.
**Método:** cada afirmação tem `ficheiro:linha`.

---

## 1. O que o katu já faz bem (não mexer)

| Mecanismo | Onde | Porque é melhor que o goose |
|---|---|---|
| **Log append-only como fonte da verdade** | `kernel/session/mod.rs`, `kernel/log.rs` | Replay byte-a-byte e `Model-visible ⟺ logged`; o goose usa SQLite + projecção ad-hoc |
| **Ordem §42 imposta** | `kernel/session/mod.rs::tool_call` | `ToolCall` logado **antes** do efeito; uma recusa nunca tem efeito |
| **Política pura e fail-closed** | `kernel/pipeline/mod.rs::dispatch_with` | A tool não é invocada num `Deny` (provado por teste) |
| **Guard de loop (CUSUM + e-value)** | `kernel/guard.rs`, `agent/turn/run.rs::cut_if_looping` | O goose só tem `max_turns`; o katu corta repetição cedo |
| **Tool calls declaradas em texto** | `agent/turn/declared.rs` | Degrau para modelos locais sem tool calls nativas |
| **Aprovação one-shot com MAC** | `agent/turn.rs::retry_with_approval` | Mais forte que o `ActionRequired` do goose |
| **Lote `Shared` paralelo com commit na ordem** | `agent/turn/batch.rs` | Fases preparar/executar/commitar preservam §42 |
| **Roteamento devolve erro ao modelo** | `agent/mod.rs::route_failure` | Argumentos maus não abortam o turno |

O objectivo **não** é trocar isto. É acrescentar os degraus que faltam.

---

## 2. Comparação peça a peça

| Dimensão | katu (hoje) | goose | Veredicto |
|---|---|---|---|
| Unidade do loop | `drive` síncrono: passo = `provider.stream` | `run_goose`: passo = `machine.step` (operação aplicável) | katu OK (event-sourced); falta uso do `StopReason` |
| Cancelamento | cooperativo, só ao chegar um evento | `CancellationToken` em cada `await` | **katu lacuna** |
| UI durante o turno | `handler.handle` bloqueia a thread da UI | agente em task; UI consome stream | **katu lacuna** |
| Teclas de cancelamento | só `Esc`; `Ctrl-C` engolido | cancelamento externo (não por tecla) | **katu lacuna** |
| `StopReason` | descartado (`stream_step` só usa `usage`) | `output_token_limit_reached` decide tool calls | **katu lacuna** |
| Tool call truncada | `ProviderError::Decode` aborta | vira erro no par `ToolResponse` | **katu lacuna** |
| Resposta vazia | `Update::Error` na TUI | retry limitado + `EMPTY_TURN_MESSAGE` visível | **katu lacuna** |
| Tecto de passos | `TooManySteps` = erro (exit 70) | `MAX_TURNS_MESSAGE` conversacional | **katu lacuna** |
| Guard de loop | erro `LoopDetected` | — | katu melhor; só falta mensagem amigável |
| `ToolCall` sem `ToolResult` | permitido no `TurnEnd`/resume | `cancellation_response` fecha os pendentes | **katu lacuna** |
| Lock de turno por sessão | não existe | `try_start_turn` | **katu lacuna** (P1) |
| Projecção para o modelo | `derive_messages` crua | `fix_conversation` (9 passos) | **katu lacuna** (P1) |
| Visibilidade dupla | log distingue controlo; sem `user/agent_visible` | `user_visible`/`agent_visible` | katu OK (o log é explícito) |
| Retry do provider | antes do 1.º delta (`retry.rs`) | antes do 1.º item (`reply_parts`) | paridade |

---

## 3. As lacunas, com evidência

### L1 — A interrupção não é um sinal de primeira classe

- O turno inteiro corre **síncrono** dentro de `Handler::handle` → `submit`
  (`crates/katu/src/tui/handler.rs:202`), chamado pelo loop da UI
  (`crates/katu-tui/src/run.rs:248`). Enquanto corre, a UI não processa eventos.
- O input só é sondado dentro de `Painter::live()` (`run.rs:57` → `poll_input` `run.rs:89`),
  chamado a cada **delta do modelo**. Sem deltas, `Esc` não é visto.
- `on_key` (`run.rs:113`) só trata `Esc`; `Ctrl-C` cai no `_ => {}` (o `KeyCode::Char(_) if control`
  é ignorado). O comentário do `handler.rs:5` promete "Esc/Ctrl-C", o código não cumpre.
- O corpo da resposta é lido **sem teto** (`http.rs:80`, `timeout_recv_body(None)`): um stream
  parado bloqueia para sempre.
- `run_calls` (`agent/turn.rs:139`) só verifica `activity.cancelled()` **entre** calls; um `bash`
  longo não é interrompível.

### L2 — Um turno interrompido deixa `ToolCall` órfão

- `run_turn_with` (`agent/turn/run.rs`) no caminho de erro faz `record_turn_end` e devolve `Err`,
  sem reconciliar calls pendentes.
- `step::turn_end` (`kernel/step/mod.rs:256`) **não exige** `pending` vazio. O estado guarda
  `pending: BTreeMap<CallId, ToolUse>` e `derive_messages` projeta o `ToolCall` mesmo sem
  `ToolResult` (`kernel/project.rs`).
- Resultado: depois de um kill/interrupção a meio, o log tem `ToolCall` sem `ToolResult` e o
  **próximo pedido ao modelo** contém uma conversa malformada (o goose corrige isto com
  `cancellation_response` + `fix_tool_calling`).

### L3 — `StopReason` é descartado

- `stream_step` (`agent/turn/run.rs:247`) usa `outcome.usage` e ignora `outcome.stop`.
- `StopReason::Length` existe (`katu-core/src/provider.rs:136`) e o decoder mapeia `"length"`
  (`katu-providers/src/openai/decode.rs:254`), mas o loop nunca o lê.

### L4 — Tool call truncada aborta o turno

- `ChatDecoder::flush_tools` (`openai/decode.rs:139`) faz `parse_arguments` e devolve
  `ProviderError::Decode` se o JSON acumulado estiver truncado. O turno termina em erro.
- O goose, em `finish_reason == "length"`, emite cada tool call como `Err(...)` no `ToolResponse`
  para o modelo se corrigir (`formats/openai.rs:1234`).

### L5 — Resposta vazia/só raciocínio é um beco sem saída

- `turn_updates` (`tui/handler.rs:249`) transforma texto vazio em `Update::Error` seco; não há
  retry nem nudge.
- Não há deteção de "só thinking" nem de eco de TOON/JSON.

### L6 — Tecto de passos e guard são erros fatais

- `TooManySteps` (`agent/mod.rs`) mapeia para `Error::internal` → exit 70.
- `LoopDetected` mapeia para `Error::conflict`. Ambos aparecem ao utilizador como falha, não como
  paragem conversacional (o goose devolve `MAX_TURNS_MESSAGE` como mensagem do assistente).

### L7 — Sem lock de turno por sessão

- O goose serializa com `try_start_turn` (`tool_confirmation_coordinator.rs:37`). O katu não tem
  equivalente: dois `katu run --resume` sobre a mesma sessão podem escrever em paralelo.

---

## 4. O que incorporar (priorizado)

### P0 — Interrupção de primeira classe

**Sintoma:** UI congelada, `Esc` não responde, `Ctrl-C` engolido, sessão quebrada após interromper.

**Mecanismo goose:** cancelamento em cada `await` + stream de eventos; `resume_state_machine_turn`
recupera o turno.

**Mudança katu (cabe no design síncrono):**

1. **Correr o turno numa thread de trabalho com `std::thread::scope`** (o padrão já existe em
   `batch.rs::in_parallel`). O `ActivitySink` deixa de ser o `Painter` e passa a um **canal**:
   - worker: `&mut Runtime`, `&provider`, `Ports`, `&options`, `Sender<Activity>`, `Arc<AtomicBool>`;
   - UI: drena o canal, chama `painter.live(...)` (que sonda input) e pinta.
   Isto sozinho elimina "UI congelada".
2. **`Esc` e `Ctrl-C` cancelam** durante o turno; um segundo `Ctrl-C` sai. `on_key` trata
   `KeyModifiers::CONTROL + Char('c')` como cancelamento (não o ignorar).
3. **`ActivitySink::cancelled()` lê o `AtomicBool`**, partilhado com o worker; o `TurnSink` e o
   `ChunkSink` verificam-no em cada evento, e `run_calls` verifica-o **dentro** de cada tool
   (passar o flag às portas; `bash` mata o filho ao cancelar).
4. **Teto de inactividade no corpo** (`http.rs`): `timeout_recv_body(Some(idle))` curto o bastante
   para acordar o loop; um `Timeout` sem cancelamento é tratado como stall recuperável (retry antes
   do 1.º delta) ou erro claro. *(Spike necessário: confirmar semântica do `ureq`.)*
5. **Reconciliar calls pendentes ao fechar o turno** (P0 porque é correcção de invariante):
   - no `TurnEnd` (ou no `Runtime::resume`, ao fechar um turno aberto), para cada `pending`,
     aplicar `ToolResult { outcome: Unavailable { control: "interrupted" }, delta: Some(...) }`;
   - alternativa mais estrita: `turn_end` **recusa** com `pending` não vazio, e o loop garante que
     o caminho de erro/cancelamento fecha todos os `begin_call`. O goose escolhe a primeira
     (`cancellation_response`).

### P0 — Fim de turno que ensina em vez de falhar

**Sintoma:** "o modelo não devolveu texto", "turno excedeu N passos", JSON cru.

**Mudança katu:**

1. **Ler `outcome.stop` em `stream_step`** e propagar em `Step`/`Accum`. Regras:
   - `StopReason::Length` + tool calls → **não** executar; emitir cada call como erro
     (`ToolOutcome::Unavailable { control: "length" }`) no par `ToolResult`, para o modelo
     reformular (equivale ao `output_token_limit_reached` do goose).
   - `StopReason::Length` + texto → mensagem visível a dizer que a resposta foi truncada e
     sugerir `--max-tokens`.
   - `ContentFilter` → mensagem explícita.
2. **Tool call truncada no decoder** (`flush_tools`): em vez de `ProviderError::Decode`, devolver um
   `ProviderEvent::ToolCall` com um marcador de erro que o loop converte em `ToolResult` de erro.
   Assim o turno não aborta por JSON incompleto.
3. **Resposta vazia**: retry limitado (ex.: 2) com nudge `agent-only` ("devolve a resposta final em
   texto"), e, no limite, uma **mensagem do assistente visível** (não `Update::Error`).
4. **`TooManySteps` e `LoopDetected`**: fechar o turno com uma mensagem do assistente que nomeia o
   que aconteceu e oferece continuar (novo turno), mantendo o erro só no envelope de máquina. O log
   fica consistente; o utilizador não vê exit 70.
5. **Deteção de eco**: se o texto final do assistente for (quase) igual a um `ToolResult.delta`
   recente, não o aceitar como resposta — nudge + retry. É a rede para o "TOON cru".

### P1 — Sessão e projeção robustas

1. **`fix_conversation` na projeção para o modelo**: antes de enviar, garantir pares
   `ToolCall`/`ToolResult` bem-formados e começar/terminar em `user` (o goose tem 9 passos em
   `conversation.rs:273`). No katu, o mínimo é: descartar `ToolCall` sem resposta e garantir ordem.
2. **Lock de turno por sessão**: um lock em `.katu/` (ou `turn_open` + PID) que recusa um segundo
   turno concorrente com erro claro — evita duas escritas no mesmo log.
3. **Resume determinístico**: já fecha turno aberto (`runtime.rs::assemble`); juntar a
   reconciliação de pendentes (P0.5) para que `Session::verify` passe após qualquer interrupção.
4. **Evento de substituição de histórico**: se a compactação muda o que o modelo vê, registar um
   evento reconstruível (o goose emite `HistoryReplaced`); hoje a compactação é aplicada no
   `context()` — confirmar que `Model-visible ⟺ logged` se mantém.

### P1 — Tools e aprovação durante a interrupção

1. Passar o `Arc<AtomicBool>` às portas para que `bash`/`process` possam terminar o filho ao
   cancelar (hoje `run_calls` só verifica entre calls).
2. Aprovação: com o worker thread, o `challenge` corre na thread da UI e o turno espera por um
   canal — deixa de bloquear o render.
3. Em `katu run` (não interactivo), manter o fail-closed actual (`approve` devolve `None`).

### P2 — Amigabilidade

1. Mensagens de erro acionáveis em todos os finais: o que aconteceu, porque, próximo passo.
2. Progresso de tools longas (spinner) e separação clara de `thinking`.
3. Custo/uso por turno já existe (`usage_line`); expor acumulado da sessão.

---

## 5. Mapa sintoma → mecanismo goose → mudança katu

| Sintoma do utilizador | Mecanismo goose | Mudança katu |
|---|---|---|
| "`Esc` não faz nada" | cancel em cada `await` | worker thread + `AtomicBool` + `poll_input` contínuo (P0.1–2) |
| "`Ctrl-C` não cancela" | idem | tratar `Ctrl-C` como cancelamento (P0.2) |
| "UI congela no `bash`" | tool futures canceláveis | flag dentro da tool / matar filho (P0.3, P1) |
| "depois de interromper, o próximo turno dá erro" | `cancellation_response` | reconciliar `pending` no `TurnEnd`/resume (P0.5) |
| "o modelo devolveu TOON/JSON cru" | `finish_reason=="length"` → erro ao modelo; `fix_conversation` | ler `StopReason`; tool truncada → erro; deteção de eco (P0.2–5) |
| "não devolveu texto" | retry + `EMPTY_TURN_MESSAGE` | retry + nudge + mensagem visível (P0.3) |
| "turno excedeu N passos" | `MAX_TURNS_MESSAGE` | paragem conversacional, não erro (P0.4) |
| "sessão quebrada após kill" | persistência a cada passo + lock | lock de turno + resume reconciliado (P1.1–3) |

---

## 6. Plano de verificação

- **Interrupção**: teste que cancela a meio de um stream (sem deltas) e prova que o turno fecha,
  que o `TurnEnd` é logado e que não há `ToolCall` sem `ToolResult`; teste de cancelamento a meio
  de um `bash` (filho terminado).
- **Fim de turno**: testes para `Length`+tool call (erro ao modelo, sem execução), `Length`+texto,
  `ContentFilter`, resposta vazia (retry e mensagem), `TooManySteps` (mensagem conversacional).
- **Sessão**: teste de resume após kill simulado (log com `ToolCall` pendente) → `verify()` verde e
  projecção bem-formada; teste de dois turnos concorrentes (segundo recusado).
- **Invariantes**: manter `cargo xtask check` (replay, `Model-visible ⟺ logged`, guard, política).
- **UI**: teste de que o loop da UI continua a processar input enquanto o worker corre.

---

## 7. O que **não** copiar do goose

- **Loop monolítico de 6k linhas** (`agent.rs`) e os **dois caminhos** legado/state-machine: o katu
  tem um caminho só e deve mantê-lo.
- **Ordenação por timestamp**: o katu usa `seq` contíguo (melhor).
- **Metadata polimórfica frouxa**: o katu tipa eventos (melhor).
- **`StateMachine<'a>` com operações que emprestam o agente**: complexidade de lifetimes
  desnecessária para o katu.

---

## 8. Referências

**Plano de implementação:** [`LOOP_RESILIENCE.md`](../../plan/LOOP_RESILIENCE.md) — forma estável de
estar (invariantes I1–I8), itens L-Q/L-P/L-S com fórmula, artefacto, teste e adotar-ou-reverter.

**katu:** `agent/mod.rs:166` (`execute_call`), `agent/turn.rs:139` (`run_calls`),
`agent/turn/run.rs:118` (`drive`), `:192` (`cut_if_looping`), `:247` (`stream_step`),
`tui/handler.rs:202` (`submit`), `:249` (`turn_updates`), `katu-tui/src/run.rs:57` (`live`),
`:89` (`poll_input`), `:113` (`on_key`), `:248` (`run`), `katu-providers/src/openai/decode.rs:139`
(`flush_tools`), `:254` (`map_stop`), `katu-providers/src/http.rs:80` (`config`),
`katu-core/src/provider.rs:136` (`StopReason`), `:170` (`ProviderOutcome`),
`katu-core/src/kernel/step/mod.rs:256` (`turn_end`), `katu-core/src/kernel/project.rs`
(`derive_messages`), `runtime.rs:159` (`resume`).

**goose:** ver [`GOOSE_LOOP`](GOOSE_LOOP.md) §7 (I/O), §4 (máquina), §6 (sessão).
