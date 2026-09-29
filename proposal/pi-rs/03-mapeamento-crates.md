# 03 — Mapeamento de Pacotes → Crates

Cada pacote npm vira um crate Rust com responsabilidade equivalente. A coluna "Fase" indica quando entrar no roadmap (ver [13-roadmap-migracao.md](./13-roadmap-migracao.md)).

| Pacote npm | Crate Rust | Responsabilidade | LOC TS | Fase |
|---|---|---:|---|---:|
| `@earendil-works/pi-telemetry` | `pi-telemetry` | Contratos de span/evento, adaptadores no-op e memória | 935 | 1 |
| `@earendil-works/pi-ai` | `pi-ai` | Providers, streaming, tools, tipos, auth, catálogo | 25.879 | 1 |
| `@earendil-works/pi-agent-core` | `pi-agent` | Agent loop, eventos, harness, sessão, compactação, tools base | 33.519 | 1–2 |
| `@earendil-works/pi-tui` | `pi-tui` | Componentes, rendering diferencial, input, clipboard nativo | 19.025 | 2 |
| `@earendil-works/pi-coding-agent` | `pi-coding-agent` | CLI, tools de arquivo/shell, extensões, modos, temas, pacotes | 76.684 | 2–4 |
| `@earendil-works/pi-durable` | `pi-durable` | Storage de conversas/tarefas/documentos, JSONL/SQLite | 11.636 | 3 |
| `@earendil-works/pi-protocol` | `pi-protocol` | Envelopes, CBOR, framing | 869 | 4 |
| `@earendil-works/pi-client` | `pi-client` | Cliente remoto | 1.135 | 4 |
| `@earendil-works/pi-server` | `pi-server` | Servidor remoto | 1.966 | 4 |
| `@earendil-works/pi-session-backend-sqlite-node` | `pi-session-sqlite` | Backend de sessão SQLite | 1.973 | 3 |
| `@earendil-works/chord` | `pi-service` (reduzido) | Serviços/RPC/estado replicado — **adiado** | 8.808 | 5 |
| `@earendil-works/pi-evals` | (manter Node) | Evals comportamentais rodando contra o binário | — | 5 |

## Detalhe por crate

### `pi-telemetry` (folha)
- Trait `TelemetryContext`, `Span`, `SpanStatus`.
- Implementações: `NoopTelemetry`, `MemoryTelemetry` (para testes).
- Esquemas tipados para spans de AI e harness (`start_ai_span`, `start_harness_span`).
- Sem dependência de I/O.

### `pi-ai`
- Tipos públicos: `Model`, `Message`, `ContentBlock`, `Usage`, `Tool`, `Context`.
- `trait Provider` com `stream`, `stream_simple`, `fetch_deferred`, `cancel_deferred`.
- `Models` registry (provider id → provider) + catálogo estático de modelos.
- Auth: env keys, credential store, OAuth (device code / PKCE), headers.
- Utils: retry, overflow, parsing de JSON parcial, event stream, UUIDv7, estimativa de tokens.
- Ver [04-camada-ai.md](./04-camada-ai.md).

### `pi-agent`
- `Agent` (estado mutável, subscribe, prompt/steer/follow_up/abort/continue).
- `agent_loop`/`agent_loop_continue` — máquina de turnos.
- Harness: system prompt, skills, prompt templates, compactação, branch summarization.
- Sessão: árvore de entradas, context builder, fork/clone, storage (memória/JSONL).
- Tools base (`read`, `write`, `edit`, `bash`, `image`) reutilizáveis por hosts.
- Ver [05-runtime-agente.md](./05-runtime-agente.md) e [10-sessoes-e-storage.md](./10-sessoes-e-storage.md).

### `pi-tui`
- `Terminal` (raw mode, tamanho, eventos), `TuiMainScreen`, `TuiAltScreen`.
- Rendering diferencial por linhas, CSI 2026 (synchronized output), bracketed paste.
- Componentes: `Text`, `Markdown`, `Editor`, `Input`, `SelectList`, `SettingsList`, `ScrollView`, `VStack`, `HStack`, `Box`, `Image`, `Loader`, `MouseRegion`.
- Cores OKLCH/OKHSL, truecolor/256, `unicode-width`.
- Imagens inline (Kitty/iTerm2).
- Ver [06-tui.md](./06-tui.md).

### `pi-coding-agent`
- CLI (`args`, auth, setup, session picker, project trust).
- Core: `SessionManager`, `SettingsManager`, `ModelRuntime`, `ResourceLoader`, `KeybindingsManager`, `EventBus`.
- Tools: `read`, `bash`, `powershell`, `edit`, `write`, `grep`, `find`, `ls` + renderers.
- Extensões: loader (jiti → trait/WASM), runner, tipos, virtual modules.
- Modos: `interactive`, `print`, `json`, `rpc`.
- Recursos: skills, prompt templates, temas, slash commands, packages.
- Ver [07-ferramentas.md](./07-ferramentas.md) e [08-cli-e-modos.md](./08-cli-e-modos.md).

### `pi-durable`
- Contratos de registros duráveis (conversa, tarefa, documento).
- Storage: memória, JSONL portátil + Node, SQLite (facade síncrona + migrações).
- Runner de conformance de storage.
- Necessário só se o runtime durável "Pico" for mantido. Para o MVP, a sessão JSONL de `pi-agent` basta.

### `pi-protocol` / `pi-client` / `pi-server`
- Envelopes versionados, alvo server/session, request/response correlacionados.
- CBOR definite-length + framing de 4 bytes BE.
- Limites padrão: 16 MiB por frame, 1M elementos, 64 níveis.
- Client e server transport-neutral (unix socket como primeira implementação).
- Ver [11-protocolo-e-servidor.md](./11-protocolo-e-servidor.md).

## Mapa de módulos `src` → módulos Rust

Exemplo para `pi-ai`:

| TS `packages/ai/src/` | Rust `crates/pi-ai/src/` |
|---|---|
| `types.ts` | `lib.rs` / `types.rs` |
| `models.ts`, `models-store.ts`, `model-catalog.ts` | `models.rs`, `catalog.rs` |
| `models.generated.ts` | `catalog/generated.rs` (gerado por xtask) |
| `providers/*.ts` | `providers/<name>.rs` |
| `api/*.ts` | `api/<name>.rs` |
| `auth/*.ts` | `auth/` |
| `oauth.ts` | `oauth.rs` |
| `utils/*.ts` | `util/*.rs` |
| `images.ts`, `image-models.ts` | `images.rs` |
| `session-resources.ts` | `session.rs` |

Exemplo para `pi-coding-agent`:

| TS `src/` | Rust `src/` |
|---|---|
| `main.ts`, `cli.ts` | `bin/pi.rs`, `cli/mod.rs` |
| `config.ts` | `config.rs` |
| `core/session-manager.ts` | `session/manager.rs` |
| `core/settings-manager.ts` | `settings.rs` |
| `core/model-runtime.ts` | `model_runtime.rs` |
| `core/tools/*.ts` | `tools/*.rs` |
| `core/extensions/*.ts` | `extensions/*.rs` |
| `modes/interactive/*.ts` | `modes/interactive/*.rs` |
| `modes/print-mode.ts` | `modes/print.rs` |
| `modes/rpc/*.ts` | `modes/rpc/*.rs` |
| `core/export-html/*` | `export_html/*` (templates embutidos) |
| `utils/*.ts` | `util/*.rs` |
