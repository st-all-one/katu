# 02 — Arquitetura Rust Proposta

## 1. Princípios de design

1. **Espelhar as fronteiras, não o código.** O monorepo TS já separa responsabilidades corretamente (`ai` → `agent` → `coding-agent`). Rust deve manter as mesmas fronteiras, com crates em vez de pacotes npm.
2. **Runtime-neutral no núcleo.** Assim como `chord`, `ai` (core) e `protocol` não dependem de Node, os crates de core não devem depender de `tokio`/`std::fs` diretamente; usam traits e features.
3. **Tipos fortes para contratos persistentes.** JSONL de sessão, envelopes CBOR e schemas de tools são contratos públicos — usar `serde`, `schemars` e enums com tag.
4. **Erros tipados.** `thiserror` no core, `anyhow` apenas nas bordas (CLI/main).
5. **Async seletivo.** `tokio` para I/O concorrente (HTTP streaming, tools paralelas, RPC); TUI usa seu próprio loop de eventos.
6. **Compatibilidade de wire/format.** Um `pi-rs` deve conseguir ler sessões JSONL do Pi original e falar o protocolo CBOR v8.
7. **Feature flags para providers e capacidades.** Não arrastar todos os providers/crates num binário mínimo.

## 2. Layout do workspace Cargo

```
pi-rs/
├── Cargo.toml                     # [workspace] members + workspace.dependencies
├── rust-toolchain.toml
├── crates/
│   ├── pi-telemetry/              # contratos de telemetria (no-op + memória)
│   ├── pi-ai/                     # API unificada multi-provider
│   │   └── src/{providers,api,auth,models,types,utils}/
│   ├── pi-agent/                  # agent loop, harness, sessão, compactação
│   ├── pi-durable/                # runtime durável (opcional)
│   ├── pi-tui/                    # biblioteca TUI (components, rendering, input)
│   ├── pi-protocol/               # envelopes + CBOR + framing
│   ├── pi-client/                 # cliente remoto
│   ├── pi-server/                 # servidor remoto
│   └── pi-coding-agent/           # CLI + tools + extensões + modos (binary `pi`)
├── xtask/                         # geração de catálogo de modelos, release, checks
└── tests/                         # testes de integração cross-crate
```

`pi-coding-agent` é `[lib] + [[bin]]`, espelhando o SDK + CLI do original.

## 3. Camadas e dependências

```
                        ┌───────────────┐
                        │ pi-telemetry  │  (folha, sem I/O)
                        └───────┬───────┘
                                │
        ┌───────────────────────┼────────────────────────┐
        ▼                       ▼                        ▼
┌───────────────┐       ┌───────────────┐        ┌───────────────┐
│   pi-ai       │◄──────│  pi-protocol  │        │ pi-durable    │
│ providers/    │       │  (chord-like) │        │ (storage)     │
│ stream/types  │       └───────┬───────┘        └───────┬───────┘
└───────┬───────┘               │                        │
        │                       ▼                        │
        │               ┌───────────────┐                │
        └──────────────►│   pi-agent    │◄───────────────┘
                        │ loop/session  │
                        └───────┬───────┘
                                │
        ┌───────────────────────┼──────────────────┐
        ▼                       ▼                  ▼
┌───────────────┐       ┌───────────────┐  ┌───────────────┐
│   pi-tui      │◄──────│pi-coding-agent│─►│  pi-client/   │
│               │       │ tools/CLI     │  │  pi-server    │
└───────────────┘       └───────────────┘  └───────────────┘
```

## 4. Mapa de tipos centrais (TS → Rust)

| Conceito TS (`pi-ai/types.ts`, `pi-agent/types.ts`) | Rust |
|---|---|
| `Message` / `AgentMessage` (union por `role`) | `enum AgentMessage` com `#[serde(tag = "role", rename_all = "camelCase")]` |
| `TextContent`/`ImageContent`/`ThinkingContent`/`ToolCall` | `enum ContentBlock` com tag `type` |
| `Model<Api>` | `struct Model` + `api: ApiId` |
| `Usage` | `struct Usage { input, output, cache_read, cache_write, ... }` |
| `AssistantMessageEventStream` | `impl Stream<Item = AssistantMessageEvent>` (`tokio_stream`/`futures`) |
| `Tool` (TypeBox schema) | `struct Tool { name, description, parameters: serde_json::Value }` + `schemars` |
| `AgentTool` (execute + renderer) | `trait AgentTool` (async) |
| `AgentEvent` | `enum AgentEvent` |
| `SessionEntry` (union) | `enum SessionEntry` com `#[serde(tag = "type")]` |
| `StreamFunction` | `trait Provider: Send + Sync` |
| `AbortSignal` | `tokio_util::sync::CancellationToken` |
| `TelemetryContext` | `trait TelemetryContext` / `Span` |

## 5. Concorrência

- **Agent loop**: task `tokio` que consome `mpsc` de mensagens e produz `broadcast`/`mpsc` de eventos. Cancelamento via `CancellationToken`.
- **Tools paralelas**: `FuturesUnordered`/`JoinSet` com gate de ordem (resultado persistido na ordem-fonte, eventos na ordem de conclusão) — reproduz o modo `parallel` do original.
- **Streaming do provider**: `reqwest` + `eventsource-stream` (SSE) ou WebSocket; normalização para `AssistantMessageEventStream`.
- **TUI**: thread/loop dedicado (ou `tokio` com `LocalSet`) lendo stdin raw e coalescendo renders — mimic do `requestRender()` coalescido.

## 6. Modelo de erros

```rust
#[derive(Debug, thiserror::Error)]
pub enum PiError {
    #[error("provider error: {0}")] Provider(#[from] ProviderError),
    #[error("tool error: {0}")]     Tool(#[from] ToolError),
    #[error("session error: {0}")]  Session(#[from] SessionError),
    #[error("protocol error: {0}")] Protocol(#[from] ProtocolError),
    #[error("io: {0}")]             Io(#[from] std::io::Error),
}
```

- Core usa enums tipados (`ProviderError`, `ToolError`, ...).
- `anyhow::Result` apenas no binário e em `xtask`.
- Erros de provider precisam carregar `status`, `retry_after`, corpo e ser convertíveis em `stopReason: error/aborted` **dentro** do stream (o contrato do Pi exige que falhas após o stream iniciado sejam codificadas no stream).

## 7. Extensibilidade

Duas opções, detalhadas em [09-extensoes-e-plugins.md](./09-extensoes-e-plugins.md):

1. **Traits + factories em Rust** (compilado, rápido, seguro) — base recomendada.
2. **WASM (wasmtime) ou scripts (rhai/lua) para extensões de usuário** — paridade com "extensões TypeScript do usuário".
3. **Processo externo via RPC** — extensões rodam em outro processo e falam o protocolo JSONL/CBOR.

A recomendação é traits nativos no MVP e WASM numa fase seguinte, com o RPC como escape hatch.

## 8. Binário e distribuição

- `cargo build --release` produz `pi` (Linux/macOS/Windows) sem runtime externo.
- Cross-compilação com `cross` ou matriz de GH Actions (musl para Linux estático, universal2 para macOS via `lipo`).
- Assets (temas JSON, docs, exemplos, template HTML de export) via `include_str!`/`include_dir!` ou embutidos no binário — remove o `copy-assets` do pipeline Node.
- Model catalog gerado em build-time por `xtask generate-models` (ver 04).

## 9. O que **não** portar

- `chord` na íntegra (facet hosts, bundler esbuild, delta replication) — só se houver o runtime remoto multi-processo. O núcleo do agente não precisa. Pode ser reduzido a um `pi-service` leve ou adiado.
- `evals` pode continuar em Node (roda contra o binário via CLI/RPC), evitando reescrever a infra de evals.
- `server`/`client` remotos: portar após o protocolo estabilizar.
