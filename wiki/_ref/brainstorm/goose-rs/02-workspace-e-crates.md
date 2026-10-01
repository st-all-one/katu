# 02 — Workspace e crates

## 1. Estrutura do workspace

O repositório é um **monorepo Cargo** com `members = ["crates/*"]` (mais `vendor/v8` apenas para o `cargo-machete`). Não há submódulos: cada pasta em `crates/` é um crate.

```toml
# Cargo.toml (raiz)
[workspace]
members = ["crates/*", "vendor/v8"]
resolver = "2"

[workspace.package]
edition = "2021"
version = "1.52.0"
rust-version = "1.94.1"
license = "Apache-2.0"

[profile.dev.package."*"]
debug = false        # remove DWARF de deps → debug build e link muito mais rápidos
```

Detalhes notáveis de engenharia:

- **Todas** as dependências são declaradas em `[workspace.dependencies]` com `default-features = false` e features explícitas. Isso é uma disciplina de **superfície mínima** e builds reprodutíveis.
- `[profile.dev.package."*"] debug = false` — terceiros sem info de debug; **apenas crates do workspace** mantêm DWARF completo.
- `[profile.lean]` (release com `opt-level="z"`, `lto="fat"`, `panic="abort"`, `strip`) para um binário mínimo.
- `[patch.crates-io]` vendoriza `v8` e fixa um `cudaforge` por rev.
- Features são usadas agressivamente para **cortar peso** (ver abaixo).

## 2. Os 15 crates e seu tamanho

LOC = arquivos `.rs` em `crates/<crate>/src`.

| Crate | LOC | Papel |
|---|---:|---|
| **goose** | 174.429 | Núcleo: agente, providers concretos, sessão, extensões, ACP, recipe, skills, hooks, permissões |
| **goose-provider-types** | 30.632 | **GDK** — trait `Provider`, `Message`/`Conversation`, formats, registry canônico |
| **goose-cli** | 28.616 | Binário `goose`: clap, sessão interativa, comandos, TUI de saída |
| **goose-providers** | 15.719 | **GDK** — implementações OpenAI/Anthropic/Google/etc. + providers declarativos |
| **goose-local-inference** | 10.347 | **GDK** — inferência local (llama.cpp, MLX, candle), tool emulation |
| **goose-mcp** | 6.084 | Servidores MCP embutidos (computercontroller, memory, tutorial…) |
| **goose-sdk-types** | 3.261 | **GDK** — tipos de custom requests/notifications ACP |
| **goose-sdk** | 2.834 | **GDK** — facade + bindings UniFFI (Python/Kotlin) |
| **goose-roaming** | 2.197 | P2P over `iroh` (identidade, pairing, relay, trust) |
| **goose-agent** | 1.510 | **GDK** — "The GDK's Agent Loop": máquina de estados genérica |
| **goose-context-management** | 1.156 | **GDK** — sumarização e compaction genéricas |
| **goose-download-manager** | 690 | **GDK** — downloads resumíveis/verificação |
| **goose-acp-macros** | 319 | Macros proc-macro para o protocolo ACP |
| **goose-test** | 258 | Utilitários de captura/replay MCP |
| **goose-test-support** | 246 | Fixtures (OTEL, sessão, MCP) |

**Total Rust:** ≈278.298 linhas.

## 3. Grafo de dependências internas

```
goose-provider-types ──┐ (base de tipos, sem deps internas)
        ▲              │
        │              ▼
   goose-agent   goose-context-management
        ▲              ▲
        │              │
   goose-providers ────┤
        ▲   ▲          │
        │   └──────────┴── goose-local-inference
        │                     ▲
   goose (núcleo) ────────────┤
     ▲   ▲                     │
     │   └── goose-mcp          │
     │        ▲                 │
   goose-cli  │                 │
     ▲        │                 │
   goose-roaming (iroh)         │
                               │
   goose-sdk ── goose-sdk-types─┘
```

Regras observadas:

- **`goose-provider-types` é a base pura**: não depende de nenhum outro crate interno. É o "contrato".
- **`goose-agent`** depende apenas de `goose-provider-types` e `rmcp` — é reutilizável fora do goose.
- **`goose`** é o "hub" que amarra providers, MCP, sessão, ACP, recipe.
- **`goose-cli`** depende de `goose` (nunca o inverso).

## 4. O que é o GDK

O **Goose Development Kit (GDK)** é o subconjunto de crates com **API pública estável**, publicado no crates.io. A fonte autoritativa é `release-plz.toml` (`release = true`, `version_group = "gdk"`):

```
goose-provider-types
goose-sdk-types
goose-download-manager
goose-local-inference   (semver_check = false)
goose-providers         (semver_check = false)
goose-agent
goose-context-management
goose-sdk
```

O `AGENTS.md` é explícito:

> "Other crates, such as `goose` and `goose-cli`, do not provide stable public APIs; their `pub` items are internal implementation details and may change without notice."

Ou seja: **existe uma fronteira deliberada entre plataforma (GDK) e aplicação (goose)**. Esse é um padrão de arquitetura de produto relevante: o núcleo reutilizável é extraído em crates publicáveis, e a aplicação é "descartável".

## 5. Features (corte de peso e capacidades opcionais)

O crate `goose` usa features para tornar capacidades opcionais:

| Feature | Efeito |
|---|---|
| `rustls-tls` / `native-tls` | mutuamente exclusivas (`compile_error!` se ambas) |
| `aws-providers` | habilita `aws-sdk-bedrockruntime`, Sagemaker |
| `local-inference` | `candle-core/nn/transformers`, `tokenizers`, `symphonia` (áudio) |
| `cuda` / `vulkan` / `mlx` | aceleração da inferência local |
| `tree-sitter` | parsers para Go/Java/JS/Kotlin/Python/Ruby/Rust/Swift/TS |
| `otel` | OpenTelemetry (traces, métricas, logs) |
| `telemetry` | PostHog |
| `code-mode` | `pctx_code_mode` |
| `live-voice` | voz em tempo real via WebSocket |

A mesma lógica aparece no CLI (`bundled-mcp`, `scheduler`, `update`, `roaming`, `acp-http`, `local-inference`).

## 6. Dependências externas centrais

| Domínio | Crate(s) |
|---|---|
| Async/run | `tokio`, `futures`, `tokio-stream`, `async-stream` |
| HTTP | `reqwest`, `axum`, `axum-server`, `tower-http` |
| Serialização | `serde`/`serde_json`, `serde_yaml`, `schemars` |
| MCP | `rmcp` (3.4.1, `schemars`, `auth`) |
| ACP | `agent-client-protocol` (+schema/http), `goose-acp-macros` |
| DB | `sqlx` (SQLite) |
| CLI | `clap` (derive) |
| Segredos | `keyring` (vendored), `winapi` (wincred) |
| TLS | `rustls` (aws_lc_rs) ou `native-tls` (openssl) |
| P2P | `iroh`, `iroh-relay` |
| Inferência local | `llama-cpp-2`, `candle-*`, `tokenizers` |
| Parsing de código | `tree-sitter-*` |
| Observabilidade | `tracing`, `opentelemetry*`, `tracing-opentelemetry` |
| Diversos | `uuid`, `chrono`, `strum`, `which`, `ignore`, `regex`, `zip` |

## 7. Lições estruturais

1. **Separar contrato de implementação.** `goose-provider-types` (puro) vs. `goose` (hub) permite que o GDK seja usado sem arrastar a aplicação.
2. **`default-features = false` em tudo.** Builds previsíveis e binários enxutos.
3. **Features como fronteira de produto** (local inference, cloud providers, voz, telemetria).
4. **Perfis de build deliberados** (`dev.package."*"` sem debug; `lean` para distribuição).
5. **Publicar o núcleo, tratar a app como interna.** Reduz o custo de evolução.
