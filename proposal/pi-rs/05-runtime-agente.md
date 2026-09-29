# 05 — Runtime do Agente (`pi-agent`)

`packages/agent` (33,5k LOC) contém o loop do agente, o estado, o harness (prompt/skills/compactação) e a sessão.

## 1. Duas camadas

1. **`Agent` / `agent-loop`** — baixo nível: estado, turnos, execução de tools, eventos.
2. **`harness/`** — alto nível: system prompt, skills, templates, compactação, runtime de execução (`harness/runtime/`), sessão e storage (`harness/session/`).

```rust
pub struct Agent {
    state: AgentState,              // model, tools, messages, thinking_level, is_streaming...
    stream_fn: Arc<dyn StreamFn>,
    convert_to_llm: ConvertToLlm,   // AgentMessage[] -> Message[]
    transform_context: Option<TransformContext>,
    before_tool_call: Option<...>,
    after_tool_call: Option<...>,
    prepare_request: Option<...>,
    finish_turn: Option<...>,
}
```

## 2. Loop de turnos (algoritmo documentado)

O loop (`agent-loop.ts`) é uma máquina de dois níveis:

```
outer loop (continua enquanto houver follow-ups enfileirados)
  inner loop (enquanto houver tool calls OU mensagens pending)
    prepareNextTurn (se houve turno anterior) -> pode trocar contexto/modelo/thinking
    emitir mensagens preparadas + steering
    prepareRequest -> pode instalar contexto canônico (ex.: recarregar do storage)
    streamAssistantResponse -> provider
    se stopReason == error/aborted -> fim (hard exit)
    executar tool calls (parallel | sequential)
    finishTurn -> { end | continue | undefined }
    turn_end
```

Regras:
- **Steering** entra depois do turno do assistente corrente; **follow-up** entra após o run atual.
- `finishTurn` roda para respostas normais, de erro e abortadas; só decisões de respostas normais são aplicadas.
- `{ action: "continue" }` força uma próxima requisição (cuidado: loop infinito se incondicional).
- `prepareRequest` roda antes de **toda** requisição, inclusive a primeira.
- Em modo paralelo: preflight sequencial, execução concorrente, `tool_execution_end` na ordem de conclusão, mas mensagens toolResult persistidas na ordem-fonte do assistente. Se **qualquer** tool do batch for `sequential`, o batch inteiro vira sequencial.
- Hooks: `beforeToolCall` (após `tool_execution_start` e parsing validado; pode bloquear e marcar `terminate`), `afterToolCall` (antes de `tool_execution_end`).
- `terminate: true` em **todos** os resultados finalizados do batch pula a chamada de follow-up. Batches mistos continuam.

## 3. Eventos

```
agent_start → turn_start → [message_start/end do user]
→ message_start (assistant) → message_update* → message_end
→ tool_execution_start → tool_execution_update* → tool_execution_end
→ message_start/end (toolResult) → turn_end → ... → agent_end
```

Tabela: `agent_start`, `agent_end`, `turn_start`, `turn_end`, `message_start`, `message_update` (só assistant), `message_end`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`.

- Listeners são aguardados em ordem de registro.
- `agent_end` = fim dos eventos do loop, mas `waitForIdle`/`prompt` só resolvem após listeners de `agent_end` terminarem.
- Nível de sessão: `agent_settled` indica que não há mais continuação automática (retries, compactação, steering, follow-up).
- Em Rust: `tokio::sync::broadcast::Sender<AgentEvent>` ou um `EventStream` com backpressure; `prompt()` retorna um future que resolve ao final do run.

## 4. Mensagens

`AgentMessage` (TS) = `SystemMessage | UserMessage | AssistantMessage | ToolResultMessage | BashExecutionMessage | CustomMessage | BranchSummaryMessage | CompactionSummaryMessage`. Extensível por declaration merging no TS.

Em Rust, `enum AgentMessage` **não é extensível** externamente. Alternativas:
- Enum fechado com todas as variantes conhecidas + `Custom { custom_type, content, display, details }` genérico (como o original faz para extensões).
- `#[non_exhaustive]` para permitir novos roles no futuro.
- Hosts que precisam de roles extras usam `Custom` com `custom_type`.

`convertToLlm` filtra UI-only e converte custom → user/assistant/toolResult. `bashExecution` vira user text (a menos que `excludeFromContext`).

## 5. System prompt evolutivo

O prompt é o **primeiro `SystemMessage`**, e mudanças posteriores são mensagens `system` que:
- fazem patch de `sections` por nome (`null` remove),
- listam `toolsAdded`/`toolsRemoved`,
- `replace: true` descarta o estado anterior e estabelece nova baseline.

Reexecutar as mensagens system em ordem reconstrói o prompt e tools atuais. Não há entrada separada de estado de prompt. Portar essa semântica é essencial para sessions compatibility.

## 6. Compactação

`harness/compaction/compaction.ts`:
- `shouldCompact`, `prepareCompaction`, `compact`, `findCutPoint`, `findTurnStartIndex`, `calculateContextTokens`, `estimateContextTokens`, `generateSummary`, `serializeConversation`.
- Defaults: `reserveTokens=16384`, `keepRecentTokens=20000`, `enabled=true` (settings).
- Gera `CompactionEntry` com `summary`, `firstKeptEntryId` (obrigatório), `tokensBefore`, `systemMessage` (checkpoint) e `usage` opcional.
- Branch summarization (`branch-summarization.ts`): resume o caminho abandonado ao trocar de branch (`BranchSummaryEntry` com `fromId`).
- Detecção de overflow (`utils/overflow.ts` do `pi-ai`) dispara recuperação.

## 7. Sessão e context building

Ver [10-sessoes-e-storage.md](./10-sessoes-e-storage.md). Resumo do `buildSessionContext`:
1. Extrai modelo/thinking do caminho ativo.
2. Converte entradas: `message` → stored message; `compaction` → checkpoint + `compactionSummary`; `branch_summary` → `branchSummary`; `custom_message` → `CustomMessage`; `usage`/`custom` → nada (não entram no contexto).
3. Compactação substitui entradas anteriores a `firstKeptEntryId`.
4. `context_edit` (append-only) altera apenas contexto futuro: `replacement: null` omite o alvo; `replacement` troca o conteúdo.

## 8. Harness runtime (`harness/runtime/`)

Subsistema que roda o agente com recuperação durável: `drive/` (boundary, checkpoint, deferred, generation, reconcile, recovery, response, retry, structural, terminal, tools), `lane.ts`, `reducer.ts`, `restore.ts`, `transcript.ts`. É o núcleo do "Pico" durável. Portar apenas se o modo durável/remoto for necessário; o MVP pode usar só `Agent` + sessão JSONL.

## 9. Tools base do agente

`harness/tools/`: `read`, `write`, `edit` (com edit-diff), `bash`, `image`, `file-mutation-queue`, `path-utils`, `tool-context`. São reutilizáveis por hosts; o `coding-agent` tem versões mais ricas com renderers.

## 10. Streaming abstraction

`stream-fn.ts` define `StreamFn` e `getDefaultStreamFn()`. `proxy.ts` envolve o stream. O `Agent` aceita `streamFn` injetável — em Rust, `Arc<dyn StreamFn>` ou genérico.

## 11. Plano de port do loop

Ordem sugerida:
1. `AgentState` + `AgentMessage` + eventos.
2. `streamAssistantResponse` com faux provider.
3. Loop simples (1 turno, 1 tool, sequencial).
4. Modo paralelo + hooks + terminate.
5. `prepareRequest`/`finishTurn`/`prepareNextTurn`.
6. `convertToLlm`/`transformContext`.
7. Compactação.
8. Sessão/context building.
9. Harness/drive durável (fase posterior).
