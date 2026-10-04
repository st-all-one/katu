# Comunicação com providers — goose, pi e katu

> **Tese.** O wire dos providers é um contrato **estrito**. O erro caro não é a forma do JSON, é a
> **forma da conversa**: um passo do assistente com N tool calls tem de ser **uma** mensagem. O
> `katu` emitia N mensagens `assistant` (uma por call) e o upstream recusava com HTTP 400.

## 0. O sintoma e a prova

Sintoma real (`katu run`, provider `opencode-go`, modelo `deepseek-v4.1-flash`):

```
indisponível: provider (HTTP 400): Upstream request failed: [invalid_request_error] invalid request
```

A mensagem completa do upstream (obtida por sonda direta ao gateway):

```
An assistant message with 'tool_calls' must be followed by tool messages responding to each
'tool_call_id'. (insufficient tool messages following tool_calls message)
```

Prova direta (mesma conversa, `stream:false`, 4 formas do histórico):

| forma do histórico | resultado |
|---|---|
| `asst(texto)`; `asst(tc1)`; `asst(tc2)`; `tool(c1)`; `tool(c2)` | **HTTP 400** (o erro acima) |
| `asst(texto, tool_calls=[tc1,tc2])`; `tool(c1)`; `tool(c2)` | OK |
| `asst(tool_calls=[tc1,tc2])`; `tool(c1)`; `tool(c2)` | OK |
| `asst(texto)`; `asst(tc1)`; `tool(c1)` (1 call) | OK |
| `asst(texto)`; `tool(c1)` (resultado sem call) | HTTP 400 («Messages with role 'tool' must be a response to a preceding message with 'tool_calls'») |

Diagnóstico: o log do katu tem **um `tool_call` por evento** (correto, um facto por call), mas o
encoder `chat/completions` emitia **um `assistant` por `Message::ToolCall`**. Com ≥2 calls no mesmo
passo, o primeiro `assistant` ficava sem os `tool` correspondentes antes do segundo `assistant`.

Nas sessões reais gravadas: **todas** as que falharam tinham `max_calls_per_step ≥ 2` (o 1.º passo
com `ls`+`read`); as que completaram tinham 1 call por passo no arranque.

## 1. goose

Lar: `_REF/goose/crates/goose-provider-types/src/formats/openai.rs` (conversão partilhada
`Message` → wire) e `_REF/goose/crates/goose-providers/src/openai_compatible.rs` (o provider).

- **Um assistant message por passo.** `format_messages_with_options` (`:212`) percorre a `Message`
  do goose (já por turno) e acumula **todos** os `ToolRequest` num só `converted["tool_calls"]`
  (`entry("tool_calls")`, `:308`/`:340`). Resultado: `assistant` com N `tool_calls`, seguido dos
  `tool` (`:396`).
- **Rede de segurança para o caso dividido.** `merge_split_tool_call_messages` (`:566`) funde
  `asst(TC)/tool` intercalados de volta num só assistant + tool results. O comentário é explícito:
  «The agent splits a single assistant response with N tool_calls into N interleaved `asst(TC)/tool`
  pairs... merges them back into one assistant message with all tool_calls, followed by the tool
  results — the standard OpenAI format.»
- **`reasoning_content` para DeepSeek/Kimi.** `preserve_thinking_context` (`:220`): o raciocínio é
  anexado ao assistant que leva tool calls e propagado entre calls do mesmo turno
  (`tool_call_turn_reasoning`, `:220`/`:495-511`); só se inclui se **não** vazio (Kimi recusa `""`).
  `inline_reasoning_content` (`:535`) cobre modelos que recusam o campo separado.
- **Tool call não parseável.** Vira placeholder `unparseable_tool_call` com `{}` (`:340`) para o
  `tool` seguinte não ficar órfão.
- **`content: null` com tool_calls** (providers estritos) e **retry** (`with_retry`, só antes do
  primeiro evento) com `handle_status` a mapear o HTTP para `ProviderError`.

## 2. pi

Lar: `_REF/pi/packages/ai/src/api/openai-completions.ts` (e os restantes `api/*`).

- **Um assistant message por passo.** `convertMessages` (`:1185`): o `AssistantMessage` do pi tem
  `content` (blocos) e o encoder emite **um** `assistant` com `content` (string) + `tool_calls` =
  todas as `ToolCall` (`:1287-1385`); o `toolResult` (`:1430`) emite `role:"tool"` com o
  `tool_call_id`.
- **Compat por provider.** `detectCompat` (`:1585`) deriva de nome/baseUrl:
  `requiresReasoningContentOnAssistantMessages: isDeepSeek` (`:1647`), `maxTokensField`
  (`max_tokens` vs `max_completion_tokens`), `supportsDeveloperRole` (não-standard ⇒ `system`),
  `requiresAssistantAfterToolResult`, etc.
- **`opencode-go` é caso especial.** pi mapeia `signature === "reasoning"` → `reasoning_content`
  (`:621`/`:1335`) e injeta `reasoning_content: ""` quando falta (`:1379`).
- **Assistente sintético.** `requiresAssistantAfterToolResult` (`:1233`) insere um `assistant`
  entre tool result e user, para providers que não aceitam user logo após tool.
- **Normalização de id.** `normalizeToolCallId` (`:1194`): ids da Responses API (`call|item`) são
  sanitizados e truncados a 40 chars.
- **Assistant vazio é saltado** (`:1385`): «either content or tool_calls, but not none».
- **Tool results consecutivos** são emitidos no mesmo laço (`:1430`).

## 3. katu

- **Log:** um `Event::ToolCall` por call; a projeção `derive_messages` mantém `Message::ToolCall`
  por call. Correto (fidelidade ao log).
- **Bug (corrigido):** `crates/katu-providers/src/openai/encode.rs` emitia um `assistant` por
  `Message::ToolCall`. Com ≥2 calls no passo → 400.
- **Fix:** `katu_core::kernel::project::wire_messages` (`:215`) agrupa a projeção — cada passo do
  assistente (texto + todas as calls) é **uma** `WireMessage::Assistant` (`:187`). O encoder OpenAI
  consome essa view (`encode_wire`, `:233`). Teste de regressão
  `openai::tests::a_step_with_two_calls_is_one_assistant_message`.
- **Verificação real:** `katu run` contra o gateway, passos com 3, 4 e 2 calls →
  `termination = natural`, exit 0.
- **Pendente (mesma classe):** os encoders **Anthropic** e **Google** ainda emitem um
  `assistant`/`model` por call e um `user` por tool result. Anthropic exige alternância de papéis e
  **um** `user` com todos os `tool_result`; Google idem para `functionResponse`. A view
  `wire_messages` já existe; falta ligá-la e agrupar os tool results por dialeto. **Responses** está
  OK (`function_call` é item de topo).

## 4. Checklist do wire (o que o katu tem de garantir)

1. Um passo do assistente = **uma** mensagem (texto + N tool calls).
2. Cada `tool_call_id` respondido **antes** do próximo assistant.
3. Nunca um `tool`/`tool_result` sem o assistant com `tool_calls` correspondente.
4. Não emitir assistant vazio.
5. `reasoning_content` quando o provider o exige (DeepSeek/Kimi) e nunca vazio.
6. Alternância de papéis onde o provider exige (Anthropic/Google) — agrupar tool results.

## 5. Referências

- goose: `_REF/goose/crates/goose-provider-types/src/formats/openai.rs` (`:212`, `:308`/`:340`,
  `:396`, `:566`); `_REF/goose/crates/goose-providers/src/openai_compatible.rs`.
- pi: `_REF/pi/packages/ai/src/api/openai-completions.ts` (`:621`, `:1185`, `:1194`, `:1233`,
  `:1287-1385`, `:1585-1647`).
- katu: `crates/katu-core/src/kernel/project.rs` (`:187`/`:215`);
  `crates/katu-providers/src/openai/encode.rs` (`:149`/`:233`).
