# 08 — CLI e Modos

## 1. Superfície de CLI (`cli/args.ts`)

Flags principais (todas devem ser reproduzidas em `clap`):

| Flag | Tipo | Efeito |
|---|---|---|
| `-h, --help` / `-v, --version` | bool | ajuda/versão |
| `--mode <text\|json\|rpc>` | enum | modo de saída |
| `-c, --continue` / `-r, --resume` | bool | retoma sessão |
| `--provider` / `--model` / `--api-key` | string | seleção de modelo |
| `--system-prompt` / `--append-system-prompt` | string/multi | prompt |
| `--thinking <level>` | enum 7 níveis | reasoning |
| `--name` / `--session` / `--session-id` / `--fork` / `--session-dir` | string | sessão |
| `--models` | lista | escopo de modelos |
| `--tools` / `--exclude-tools` / `--no-tools` / `--no-builtin-tools` | listas/bool | tools |
| `--extensions` / `--no-extensions` | lista/bool | extensões |
| `--skills` / `--no-skills` | lista/bool | skills |
| `--prompt-templates` / `--no-prompt-templates` | lista/bool | templates |
| `--themes` / `--use-theme` / `--no-themes` | lista/bool | temas |
| `--no-context-files` | bool | AGENTS.md/contexto |
| `--list-models [pattern]` | string/bool | lista modelos |
| `--offline` | bool | não busca catálogo |
| `--tui-mode` | enum | layout TUI |
| `--verbose` | bool | debug |
| `--project-trust-override` | bool | trust |
| `--print` / `-p` | bool | modo print |
| `--export <file>` | string | exporta sessão |
| posicional `@file` / mensagens | | arquivos/mensagem inicial |

Flags desconhecidas são coletadas em `unknownFlags` (extensões podem registrar flags). Em `clap`, usar `allow_hyphen_values` + `trailing_var_arg` e um fallback para coletar desconhecidas.

## 2. Resolução de modo

```
--mode rpc        -> RpcMode
--mode json       -> JsonMode
--print / !tty    -> PrintMode
caso contrário    -> InteractiveMode
```

## 3. Modos de interface (`modes/`)

### Interactive (`interactive/`)
- `interactive-mode.ts` orquestra o TUI; `tui-renderer.ts` liga eventos do agente a componentes.
- `chat-viewport.ts` é o transcript (scroll, seleção, busca).
- Componentes: `assistant-message`, `user-message`, `tool-execution`, `diff`, `footer`, `custom-editor`, `model-selector`, `session-selector`, `tree-selector`, `thinking-selector`, `theme-selector`, `settings-selector`, `login-dialog`, `oauth-selector`, `trust-selector`, `mermaid`, `pi-logo`, etc.
- Temas em `theme/` (JSON) + `theme.ts`/`theme-json.ts`.
- Slash commands: `/settings /model /tree /thinking /scoped-models /export /import /share /bug /copy /name /session /changelog /hotkeys /fork /clone /trust /login /logout /new /compact /resume /reload /quit`.
- Keybindings configuráveis; editor customizado preserva atalhos da app.

### Print (`print-mode.ts`)
- Executa um prompt e escreve a resposta final. Usado com pipe (`!tty`) automaticamente.

### JSON (`json-event.ts`)
- Escreve **eventos do agente** como JSONL para stdout.

### RPC (`rpc/`)
- JSONL de comandos em stdin, respostas e eventos em stdout.
- Framing **estrito JSONL**: um objeto por linha, terminado em `\n`; aceitar CRLF; **não** usar line reader que quebre em U+2028/U+2029 (Node `readline` faz isso — em Rust, um splitter por byte `\n` explícito).
- Comandos: `prompt`, `steer`, `follow_up`, `abort`, `clear_queue`, `new_session`, `get_state`, `get_messages`, `set_model`, `cycle_model`, `get_available_models`, `set_thinking_level`, `cycle_thinking_level`, `get_available_thinking_levels`, `set_steering_mode`, `set_follow_up_mode`, `compact`, `set_auto_compaction`, `set_auto_retry`, `abort_retry`, `bash`, `abort_bash`, `get_session_stats`, `export_html`, `switch_session`, `fork`, `clone`, `get_fork_messages`, `get_entries`, `get_tree`, `get_last_assistant_text`, `set_session_name`, `get_commands`.
- `data.disposition` do `prompt`: `started` | `queued` | `handled`. Se `handled`, não esperar `agent_settled`.
- Eventos: `message_update` (com `assistantMessageEvent`), `tool_execution_*`, `compaction_*`, `agent_end`, `agent_settled`, etc.
- `extension_ui_request`/`extension_ui_response` para diálogos de extensão (ver `docs/rpc-extension-ui.md`).
- Respostas: `{ id, type:"response", command, success, data?, error? }`.
- Backpressure: honrar stdin ao escrever; ler stdout continuamente; stderr é só diagnóstico.
- Shutdown: fechar stdin → dispose do runtime.

## 4. SDK (in-process)

No original é `createAgentSession()` retornando `AgentSession` com `prompt/steer/followUp/abort/waitForIdle/subscribe/dispose`, e `AgentSessionRuntime` com `newSession/switchSession/fork/importFromJsonl`. Em Rust, o "SDK" é a própria API pública do crate `pi-coding-agent`:

```rust
let mut session = AgentSession::builder().cwd(".").build().await?;
let mut events = session.subscribe();
tokio::spawn(async move { while let Ok(ev) = events.recv().await { /* ... */ } });
session.prompt("What files are here?").await?;
println!("{}", session.last_assistant_text().unwrap_or_default());
session.dispose().await;
```

`session.system_prompt` é read-only e reflete mudanças ainda não enviadas. `SessionManager` é autoritativo para o contexto finalizado: restaurar histórico externo = construir sessão com um manager contendo as entradas; atribuir `state.messages` diretamente **não** substitui o contexto persistido.

## 5. Configuração e settings

- Diretório do agente: `~/.pi/agent` (config `~/.pi/agent/settings.json`?) e settings do projeto; projeto sobrepõe.
- Settings: modelo/thinking, interação (`steeringMode`, `followUpMode`, `externalEditor`, `doubleEscapeAction`, `treeFilterMode`, `defaultProjectTrust`), tools (`defaultTools`), sessões (`sessionDir`), compactação (`enabled`, `reserveTokens`, `keepRecentTokens`, `modelOverrides`), cache warm, etc. Ver `docs/settings.md`.
- Em Rust: `serde` com defaults e merge (agent-dir → project). Preservar nomes de campos JSON para compatibilidade.
- Project trust: resolve antes de carregar settings/recursos do projeto; `ask|always|never`.

## 6. Recursos descobertos

- **Context files** (AGENTS.md etc.), **skills**, **prompt templates**, **themes**, **extensions**, **packages** (npm/git).
- Descoberta e carga no `ResourceLoader`. Trust controla se recursos do projeto são carregados.
- Packages: distribuição via npm/git; em Rust, suportar um diretório de pacote com manifest (JSON) + recursos, sem necessariamente reinstalar via npm.

## 7. Export HTML

- `core/export-html/`: `template.html`, `template.css`, `template.js` + `highlight.min.js`/`marked.min.js` vendorizados; `ansi-to-html.ts`, `tool-renderer.ts`.
- Em Rust: embutir os templates com `include_str!` e reimplementar `ansi-to-html` + `tool-renderer`. O JS do vendor pode ser mantido embutido (o HTML é autocontido).

## 8. Ordem de port

1. `parseArgs` (clap) + `--help`/`--version`.
2. `PrintMode` (sem TUI) — primeiro alvo executável.
3. `JsonMode`.
4. `RpcMode` (habilita integrações e testes).
5. `InteractiveMode` (depende do `pi-tui`).
6. Export HTML, list-models, setup/auth, session picker, trust.
