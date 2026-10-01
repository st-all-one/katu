# 13 — Roadmap de Migração

O plano segue fatias verticais (end-to-end utilizáveis) em vez de portar camada por camada até o fim. Cada fase termina com um artefato executável e testável.

## Visão geral

| Fase | Objetivo | Entregável | Depende de |
|---|---|---|---|
| 0 | Fundação do workspace | `cargo` workspace, CI, lint, xtask | — |
| 1 | Núcleo AI + loop mínimo | `pi-ai` (3 APIs) + `pi-agent` com faux provider | 0 |
| 2 | CLI print/JSON + sessão | `pi` não-interativo com `read/write/edit/bash`, sessão JSONL | 1 |
| 3 | TUI + modo interactive | `pi-tui` + `InteractiveMode` | 2 |
| 4 | Extensões nativas + recursos | traits, skills, templates, temas, packages | 3 |
| 5 | Providers amplos + auth | OAuth, Bedrock/Vertex/Responses, catálogo gerado | 1 |
| 6 | Modo RPC + remoto | `RpcMode`, `pi-protocol`/client/server | 2, 5 |
| 7 | Durável (opcional) | `pi-durable`, workers, estado replicado | 6 |

## Fase 0 — Fundação

- Criar workspace `crates/*`, `rust-toolchain.toml`, `deny.toml`.
- CI: `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo audit`.
- `xtask` com `generate-models` (stub) e `check`.
- Portar `pi-telemetry` (noop + memória).
- **Saída**: workspace vazio compilando com lint limpo.

## Fase 1 — Núcleo AI + loop mínimo

- `pi-ai`: tipos (`Model`, `Message`, `ContentBlock`, `Usage`, `Tool`, `Context`), `EventStream`, `faux` provider, retry, overflow, uuid/estimate.
- `pi-agent`: `AgentState`, `AgentMessage`, eventos, `streamAssistantResponse` com faux provider, loop de 1 turno + 1 tool sequencial.
- Providers reais: OpenAI-compatible (chat completions), Anthropic Messages, Google Generative AI.
- `pi-agent`: modo paralelo + hooks + terminate + `finishTurn`/`prepareRequest`.
- Testes: deterministic faux provider; golden de eventos.
- **Saída**: agente conversa com tool em teste, sem rede, e com 3 providers reais.

## Fase 2 — CLI print/JSON + sessão + tools

- `pi-coding-agent`: `clap` args, `PrintMode`, `JsonMode`.
- Tools: `read`, `write`, `edit` (edit-diff + fila), `bash` (truncation/accumulator).
- `SessionManager` + JSONL em disco + `buildContext` + migrações.
- System prompt builder + context files (AGENTS.md).
- Auth por env + credential store.
- **Saída**: `pi -p "..."` funcional em CI; sessões legíveis pelo Pi original.

## Fase 3 — TUI + interactive

- `pi-tui`: terminal, diff, main-screen, componentes básicos.
- `Editor`/`Input`, keybindings, markdown, syntax highlight.
- `InteractiveMode`, chat viewport, footer, seletores (model/session/theme/thinking).
- Alt-screen + ScrollView + overlays + mouse (portar o `tui-plan.md`).
- Imagens inline + clipboard.
- **Saída**: experiência interativa comparável ao Pi.

## Fase 4 — Extensões nativas + recursos

- `Registry` + traits `Extension`, hooks, UI context.
- Skills, prompt templates, temas (parser + loader).
- Builtin extensions portadas.
- Project trust + package manager simplificado.
- Export HTML.
- **Saída**: extensões Rust de primeira parte + recursos declarativos.

## Fase 5 — Providers amplos + auth

- `xtask generate-models` gerando catálogo a partir de fontes upstream.
- Adapters: OpenAI Responses/Azure/Codex, Bedrock Converse, Vertex, Mistral, xAI/Groq/Cerebras/OpenRouter (via OpenAI-compat), etc.
- OAuth: GitHub Copilot, Codex, Vertex, Radius (device code + PKCE).
- Cache warming, prompt cache lifetimes, deferred responses.
- **Saída**: paridade de providers próxima de 100%.

## Fase 6 — RPC + remoto

- `RpcMode` completo (todos os comandos/eventos).
- `pi-protocol` + `pi-client` + `pi-server` (unix), depois TCP/TLS.
- Extension UI sobre RPC.
- `RpcClient` equivalente.
- **Saída**: integrações externas e IDE.

## Fase 7 — Durável (opcional)

- `pi-durable`: contratos, storage memory/JSONL/SQLite, conformance.
- Harness `drive/` com checkpoint/recovery/retry.
- Workers de sessão + coordenação.
- Estado replicado/`chord` se houver caso de uso.
- **Saída**: agentes duráveis multi-processo.

## Marcos de compatibilidade

| Marco | Critério |
|---|---|
| C1 | `pi-rs` abre uma sessão JSONL v3 gerada pelo Pi original sem perda |
| C2 | Pi original abre uma sessão gerada por `pi-rs` |
| C3 | Eventos JSON/RPC têm os mesmos nomes e campos |
| C4 | Um `RpcClient` existente fala com `pi-rs` |
| C5 | Protocolo CBOR v8 interoperável |

## Estimativas de esforço (ordem de grandeza)

| Fase | Esforço relativo |
|---|---|
| 0 | 2% |
| 1 | 15% |
| 2 | 20% |
| 3 | 22% |
| 4 | 12% |
| 5 | 15% |
| 6 | 8% |
| 7 | 6% (opcional) |

Considerar ~1,5–2,5x reescrita efetiva vs. 185k LOC TS por conta de testes, paridade de strings de prompt e edge cases.

## Estratégia de convivência

- Manter o Pi original rodando como referência/oráculo durante todo o port.
- Testes de paridade comparam outputs de ambos (mesmo input, faux provider determinístico).
- Evals (Node) rodam contra os dois binários para medir regressão comportamental.
