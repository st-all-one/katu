# 01 — Análise do Codebase

Fonte analisada: `_REF/pi` (branch `main`, commit `cb7969d`, versão dos pacotes `0.87.1`, versão raiz `0.0.3`).

## 1. Identidade

- **Projeto**: Pi Agent Harness — "self extensible coding agent".
- **Site**: pi.dev · **Docs**: `packages/coding-agent/docs/`
- **Licença**: MIT, Copyright (c) 2025 Mario Zechner.
- **Runtime**: Node.js `>=22.19.0`, ESM, TypeScript "erasable syntax only" (strip-only mode).
- **Origem**: npm `@earendil-works/*`.

## 2. Estrutura do monorepo

```
_REF/pi/
├── packages/
│   ├── chord/                     # runtime de composição de aplicação (plugins, RPC, estado replicado)
│   ├── telemetry/                 # contratos de telemetria agnósticos de vendor
│   ├── ai/                        # API unificada multi-provider de LLM
│   ├── durable/                   # runtime durável (conversas, tarefas, documentos) — "Pico"
│   ├── agent/                     # runtime do agente + harness + sessão
│   ├── coding-agent/              # CLI interactivo
│   ├── tui/                       # biblioteca de terminal UI
│   ├── protocol/                  # protocolo CBOR remoto
│   ├── client/                    # cliente CBOR remoto
│   ├── server/                    # servidor CBOR remoto
│   ├── evals/                     # evals comportamentais (vitest-evals)
│   └── session-backends/
│       └── sqlite-node/           # backend de sessão SQLite (node:sqlite)
├── scripts/                       # build, release, model catalog, shrinkwrap, checks
├── .pi/                           # recursos do próprio projeto (extensões, prompts, skills)
├── package.json                   # workspaces + scripts de build/check/release
├── tsconfig.base.json / tsconfig.json
├── tui-plan.md                    # design do layout alt-screen (36KB)
├── AGENTS.md / CONTRIBUTING.md / SECURITY.md
└── README.md
```

## 3. Métricas de código-fonte

Contagem de arquivos `.ts` em `packages/*/src` e linhas totais:

| Pacote | Arquivos `.ts` | Linhas |
|--------|---------------:|-------:|
| `ai` | 191 | 25.879 |
| `agent` | 117 | 33.519 |
| `coding-agent` | 279 | 76.684 |
| `tui` | 44 | 19.025 |
| `chord` | 29 | 8.808 |
| `durable` | 37 | 11.636 |
| `protocol` | 8 | 869 |
| `client` | 8 | 1.135 |
| `server` | 16 | 1.966 |
| `telemetry` | 6 | 935 |
| `sqlite-node` | 15 | 1.973 |
| **Total** | **~750** | **~184.471** |

> Não inclui testes, docs (`packages/coding-agent/docs` tem 50+ arquivos), exemplos e o `tui-plan.md`.

## 4. Grafo de dependências (workspace)

```
chord ──────────────┐
telemetry ──────────┼──> ai ──────────┐
                    │                 ├──> agent ──┐
durable ────────────┘                 │            ├──> coding-agent ──> tui
                                      │            │
protocol ──> chord                    └────────────┘
client  ──> protocol, chord
server  ──> protocol, agent, chord
sqlite-node ──> ai, agent
evals ──> (usa o CLI compilado)
```

Regras observadas:
- `chord`, `telemetry` e `protocol` são runtime-neutral (sem Node APIs no core).
- `ai` é a base de tipos e providers; `agent` depende de `ai` + `chord` + telemetry.
- `coding-agent` é a única camada que conhece filesystem, processo, rede e UI.

## 5. Fluxo de execução (resumo)

```
CLI (main.ts) → parseArgs → createAgentSession(SDK)
   → ModelRuntime (auth + models)
   → SessionManager (JSONL em árvore)
   → Agent (agent-core)
        → agentLoop: turnos → stream do provider → tool calls → tool results
        → eventos (agent/turn/message/tool)
   → modo de saída:
        interactive (TUI) | print | json | rpc
```

Detalhes em how-pi-works.md (`_REF/…`, projecto externo; não vive neste repositório).

## 6. Conceitos centrais reproduzíveis

1. **`AgentMessage` vs `Message`** — o agente opera com mensagens extensíveis (`bashExecution`, `custom`, `branchSummary`, `compactionSummary`); os providers só entendem role `system|user|assistant|toolResult`, feita a ponte por `convertToLlm`.
2. **Sessão = árvore** — entradas JSONL com `id`/`parentId`; branch ativo define o contexto; compactação e `context_edit` alteram apenas o contexto futuro.
3. **System prompt evolutivo** — mensagens `system` são deltas (patch de `sections` por nome, `toolsAdded`/`toolsRemoved`), então o histórico reexecutado reconstrói o prompt atual.
4. **Tools como cidadãos de primeira classe** — cada tool tem schema (TypeBox), renderer de TUI, modo de execução (`parallel`/`sequential`), hooks `beforeToolCall`/`afterToolCall`.
5. **Extensões em processo** — módulos TS carregados via `jiti`, capazes de registrar tools, comandos, atalhos, providers, UI, renderers e handlers de eventos.
6. **Modos de interface ortogonais** — todos os modos usam o mesmo agente e a mesma sessão.

## 7. Dependências externas (candidatas a substituição)

Ver [12-dependencias-rust.md](12-dependencias-rust.md). Resumo por categoria:

- **SDKs de provider**: `openai`, `@anthropic-ai/sdk`, `@google/genai`, `@aws-sdk/client-bedrock-runtime`, `@smithy/node-http-handler`
- **HTTP/proxy**: `undici`, `http-proxy-agent`, `https-proxy-agent`
- **Schema/JSON**: `typebox`, `partial-json`
- **TUI/terminal**: `chalk`, `get-east-asian-width`, `marked`, `highlight.js`, `grok-mermaid`
- **Arquivos/glob**: `ignore`, `minimatch`, `diff`
- **Processo/execução**: `cross-spawn`, `jiti`, `proper-lockfile`, `semver`, `hosted-git-info`, `yaml`
- **Imagem**: `@silvia-odwyer/photon-node` (que já é Rust/WASM por baixo)
- **Bundling (chord)**: `esbuild`

## 8. Código nativo

`packages/tui/native/` traz addons N-API por plataforma:

| Plataforma | Arquivo | Função |
|-----------|---------|--------|
| Darwin | `darwin-platform.m` (ObjC/AppKit) | estado de modificadores, clipboard texto/imagem/path |
| Linux | `linux-platform-x11.c` (libxcb) | leitura assíncrona X11 de clipboard |
| Win32 | `win32-platform.c` (kernel32/user32) | setup de console, modificadores, clipboard |

Em Rust isso deixa de ser necessário: clipboard e detecção de modificadores são via crates (`arboard`, `crossterm`/`ratatui`), removendo prebuilds e `build:native:*`.

## 9. Build, empacotamento e distribuição

- `npm run build` encadeia `tsc` por pacote; `coding-agent` gera bundle via esbuild.
- `scripts/build-binaries.sh` usa `bun build --compile` para produzir tarballs por plataforma (darwin/linux/windows, x64/arm64) com assets (temas, docs, exemplos, wasm do photon).
- `packages/coding-agent/npm-shrinkwrap.json` fixa transitivas para usuários npm.
- Supply-chain: deps diretas pinadas exatas, `min-release-age=2`, lockfile como ground truth.

Em Rust, `cargo build --release` + `cross`/GitHub Actions substituem esse pipeline; o binário é estático-ish por natureza.

## 10. Documentação relevante embutida

- `docs/how-pi-works.md` — arquitetura de sessão/loop/contexto.
- `docs/session-format.md` — contrato JSONL (v1→v3), tipos de entrada.
- `docs/message-types.md` — `AgentMessage`, content blocks, usage.
- `docs/sdk.md`, `docs/rpc.md`, `docs/rpc-commands.md`, `docs/json.md` — APIs de integração.
- `docs/extensions.md`, `docs/tui.md`, `docs/themes.md`, `docs/skills.md`, `docs/prompt-templates.md`.
- `docs/providers.md`, `docs/models.md`, `docs/settings.md`, `docs/configuration.md`.
- `tui-plan.md` — design interno do sistema de layout alt-screen.
