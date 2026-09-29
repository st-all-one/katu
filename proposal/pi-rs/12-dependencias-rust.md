# 12 — Dependências npm → Crates Rust

Tabela de substituição. "Alternativa" é a recomendação primária; crates secundárias entre parênteses.

## 1. SDKs de provider

| npm | Rust |
|---|---|
| `openai` | `reqwest` + adapter OpenAI-compatible próprio (SSE) |
| `@anthropic-ai/sdk` | `reqwest` + adapter Anthropic Messages (SSE) |
| `@google/genai` | `reqwest` + adapter Google Generative AI / Vertex |
| `@aws-sdk/client-bedrock-runtime` | `aws-sdk-bedrockruntime` + `aws-config` (ou `aws-sigv4` manual) |
| `@smithy/node-http-handler` | n/a (reqwest) |
| `http-proxy-agent`, `https-proxy-agent` | suporte de proxy do `reqwest` (`Proxy`) |

## 2. HTTP / streaming

| npm | Rust |
|---|---|
| `undici` | `reqwest` (rustls) + `hyper` |
| SSE | `eventsource-stream` ou parser SSE próprio |
| WebSocket | `tokio-tungstenite` |
| JSON parcial | `json-event-parser` / parser incremental sobre `serde_json` |
| retry/backoff | `backoff` ou próprio |

## 3. Schema / JSON / YAML

| npm | Rust |
|---|---|
| `typebox` | `serde` + `schemars` (gerar JSON Schema) + `jsonschema` (validar) |
| `partial-json` | ver acima |
| `yaml` | `serde_yaml` (ou `serde_yml`, `saphyr`) |
| `partial-json` | ver acima |
| `diff` | `similar` (diff de texto) / `similar-asserts` nos testes |

## 4. TUI / terminal

| npm | Rust |
|---|---|
| `chalk` | `owo-colors` / `anstyle` |
| `get-east-asian-width` | `unicode-width` |
| `marked` | `pulldown-cmark` / `comrak` |
| `highlight.js` | `syntect` (ou `tree-sitter-highlight`) |
| `grok-mermaid` | sem equivalente direto; renderizar texto/ASCII ou embutir renderer (avaliar) |
| terminal I/O | `crossterm` |
| clipboard | `arboard` (X11/Wayland/Win/macOS) |
| input | `crossterm::event` + `unicode-segmentation` |
| emulador headless (testes) | `vt100` / `avt` |

## 5. Arquivos / glob / ignore

| npm | Rust |
|---|---|
| `ignore` | `ignore` (mesma origem, ripgrep) |
| `minimatch` | `globset` (+ `glob`) |
| `proper-lockfile` | `fs2` / `fd-lock` |
| `semver` | `semver` |
| `hosted-git-info` | `git-url-parse` / parse próprio |
| `cross-spawn` | `std::process::Command` / `tokio::process::Command` |

## 6. Imagem / mídia

| npm | Rust |
|---|---|
| `@silvia-odwyer/photon-node` (WASM) | `image` (resize, encode) |
| canvas (evals) | n/a |

## 7. Runtime / extensões / bundling

| npm | Rust |
|---|---|
| `jiti` (loader TS) | `wasmtime`/`wasmi` (WASM) ou `mlua`/`rhai`; ver [09](./09-extensoes-e-plugins.md) |
| `esbuild` (chord bundler) | n/a no MVP; `wasm-bindgen`/`wasmtime` se portar chord |
| `typebox` | ver §3 |

## 8. CLI / config / misc

| npm | Rust |
|---|---|
| (parsing de args) | `clap` (derive) |
| `chalk` | ver §4 |
| `semver` | `semver` |
| `undici` | `reqwest` |
| `proper-lockfile` | `fs2` |
| UUIDv7 | `uuid` (feature `v7`) |
| hashing | `sha2`, `blake3` |
| `hosted-git-info` | parse próprio |

## 9. Storage / protocolo

| npm | Rust |
|---|---|
| `node:sqlite` (sqlite-node) | `rusqlite` (feature `bundled`) ou `sqlx` |
| CBOR (protocol) | `ciborium` / `minicbor` |
| unix sockets | `tokio::net::UnixStream` |
| TLS | `rustls` + `tokio-rustls` |

## 10. Observabilidade / telemetria

| npm | Rust |
|---|---|
| (telemetry própria) | `tracing` + `tracing-subscriber` (implementar o schema próprio sobre tracing/OTel) |
| (OTel não usado diretamente no core) | `opentelemetry` se necessário |

## 11. Testes / dev

| npm | Rust |
|---|---|
| `vitest` | `cargo test` + `tokio::test` |
| `@xterm/headless` | `vt100` / `avt` |
| `vitest-evals` | manter em Node (evals) ou `insta` para snapshots |
| coverage | `cargo-llvm-cov` (`@vitest/coverage-v8`) |
| lint/format | `cargo clippy` + `rustfmt` (no lugar de biome) |

## 12. Tabela-resumo das crates principais

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
jsonschema = "0.18"
crossterm = "0.28"
unicode-width = "0.2"
unicode-segmentation = "1"
pulldown-cmark = "0.12"
syntect = "5"
similar = "2"
ignore = "0.4"
globset = "0.4"
clap = { version = "4", features = ["derive"] }
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v7", "v4"] }
rusqlite = { version = "0.32", features = ["bundled"] }
ciborium = "0.2"
arboard = "3"
image = "0.25"
sha2 = "0.10"
fs2 = "0.4"
eventsource-stream = "0.2"
tokio-tungstenite = "0.24"
```

## 13. Observações de supply chain

- O original pina deps diretas exatas. Em Rust, usar `Cargo.lock` commitado + `cargo-deny`/`cargo-audit` em CI (equivalente ao `npm audit` agendado).
- `cargo vet` ou `cargo-crev` para revisão de dependências novas.
- Evitar crates com build scripts pesados quando houver alternativa; `rusqlite` com `bundled` compila C (aceitável, remove dependência de sistema).
