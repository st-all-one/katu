# 11 — GDK e SDK

## 1. O que é o GDK

> "The goose Development Kit (GDK) provides the core components developers need to build agentic applications, including agent orchestration, model access, context management, tools, memory, remote execution, automations, and routing. There are two ways to use it:
> - **SDK** — use goose's provider layer as an in-process library from Rust, Python, or Kotlin.
> - **ACP** — connect to goose as a separate agent process over stdio, HTTP, or WebSocket."

O GDK é, portanto, **a fronteira de produto**: o que o goose publica para terceiros. O resto (`goose`, `goose-cli`) é interno.

## 2. A lista autoritativa de crates GDK

Definida em `release-plz.toml` (`release = true`, `version_group = "gdk"`), com versionamento conjunto `0.1.0-alpha.11`:

| Crate | Papel no GDK |
|---|---|
| `goose-provider-types` | Contrato: `Provider`, `Message`, `Conversation`, formats, registry canônico |
| `goose-sdk-types` | Tipos wire ACP customizados (requests/notifications) |
| `goose-download-manager` | Downloads resumíveis/verificados |
| `goose-local-inference` | Inferência local (llama.cpp/MLX/candle) |
| `goose-providers` | Implementações de providers |
| `goose-agent` | **O loop do agente** (máquina de estados genérica) |
| `goose-context-management` | Compaction/sumarização genéricas |
| `goose-sdk` | Facade + bindings UniFFI |

O `AGENTS.md` reforça que `goose` e `goose-cli` **não** têm API pública estável.

## 3. Arquitetura em camadas do GDK

```
goose-provider-types   ← contrato puro (sem deps internas)
      ▲         ▲
      │         │
goose-agent  goose-context-management
      ▲         ▲
      └────┬────┘
           │
      goose-providers  ←── goose-local-inference
           ▲
           │
     goose-sdk-types
           ▲
           │
       goose-sdk  ←── bindings UniFFI (Python/Kotlin)
```

A regra é que **cada camada depende só da(s) de baixo**. `goose-agent` conhece apenas `goose-provider-types`.

## 4. `goose-sdk` — in-process via UniFFI

`crates/goose-sdk/src/lib.rs`:

> "With default features this crate re-exports the shared GDK wire types from `goose-sdk-types` so you can build an Agent Client Protocol (ACP) client that talks to `goose acp` over stdio.
>
> With `--features uniffi` the crate additionally compiles as a `cdylib`/`staticlib` and exposes an in-process API to Python and Kotlin via uniffi-rs. The current uniffi surface lets callers construct declarative providers from JSON and stream provider completions."

```toml
[lib]
crate-type = ["cdylib", "staticlib", "rlib"]

[[bin]]
name = "goose-uniffi-bindgen"
required-features = ["uniffi"]
```

### Superfície UniFFI (`bindings.rs`, 2.289 linhas)

Tipos exportados (`#[uniffi::export]` / `#[derive(uniffi::Record/Enum)]`):

- `GooseError`, `GooseStreamError`
- `ProviderMessage`, `MessageRole`, `MessageContent`
- `ProviderTool`
- `ProviderModelConfig`, `ProviderModelInfo`
- `Usage`, `StreamChunk`
- `RequestLogger` (callback interface) + `install_request_logger(...)`

Ou seja, a superfície atual é deliberadamente **pequena**: construir providers declarativos a partir de JSON e **streamar completions**. O agente completo é consumido via ACP, não in-process.

### `observability.rs`

Expõe hook de observabilidade para os bindings (520 linhas), integrando logging/telemetria ao logger instalado pelo host.

### Empacotamento

`crates/goose-sdk/scripts/`: `gdk-release.py`, `prepare-maven-package.sh`, `maven-resource-prefix.sh` — há publicação para o ecossistema JVM (Kotlin) além do PyPI.

## 5. `goose-agent` — o loop como biblioteca

O crate que dá nome ao "agent loop" do GDK. Já detalhado no doc 04. Pontos de API pública:

- `StateMachine`, `Step`, `Operation`, `Inference`, `OperationResult`, `StepResult`, `MachineEffect`, `ConversationEffect`, `Emitter`.
- Traits de runtime: `MachineSession`, `SessionLoader`, `EffectHandler`, `EffectUsage`.
- `InferenceRunner`, `InferenceRequestPreparer`, `InferenceEffect`, `ToolProvider`, `ToolOperation`.
- `AgentEvent`.

É reutilizável por **qualquer** aplicação que implemente os traits de sessão/efeito e forneça operações — não apenas o goose.

## 6. `goose-sdk-types` — o contrato de protocolo

- `custom_requests.rs` (2.374) — métodos ACP customizados.
- `custom_requests/recipe.rs`, `custom_requests/schedule.rs`.
- `custom_notifications.rs` (285).

Com schemas gerados via `schemars`. É o "contrato compartilhado" entre servidor goose, desktop e clientes ACP.

## 7. Duas formas de integração

| | **SDK (in-process)** | **ACP (out-of-process)** |
|---|---|---|
| Linguagem | Rust, Python, Kotlin | qualquer (stdio/HTTP/WS) |
| Superfície | provadores declarativos + streaming | agente completo |
| Acoplamento | link na mesma lib | processo separado |
| Uso típico | embed de acesso a modelos | editor/integração de agente |

## 8. Lições

1. **Fronteira pública deliberada**: publicar apenas o núcleo reutilizável.
2. **Versionamento conjunto** (`version_group`) simplifica consumo.
3. **UniFFI** viabiliza Python/Kotlin sem reescrever — e com uma superfície mínima e clara.
4. **O loop do agente é uma biblioteca** (`goose-agent`), não um detalhe da aplicação.
5. **Contrato de protocolo em crate próprio** com schemas gerados.
6. **Dois modos complementares** (in-process para providers; out-of-process para o agente).
