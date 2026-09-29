# 14 — Testes e qualidade

## 1. Escala

| Crate | `#[test]`/`#[tokio::test]` |
|---|---:|
| `goose` | 2.457 |
| `goose-provider-types` | 612 |
| `goose-cli` | 311 |
| `goose-providers` | 270 |
| `goose-mcp` | 85 |
| **Total (workspace)** | **≈3.972** |

Além dos testes unitários inline, há `tests/` de integração em 7 crates (`goose`, `goose-cli`, `goose-providers`, `goose-provider-types`, `goose-local-inference`, `goose-roaming`, `goose-agent`), com 35 arquivos só em `crates/goose/tests/`.

## 2. Ferramentas de teste

Do `[workspace.dependencies]` e dev-deps:

| Crate | Uso |
|---|---|
| `wiremock` | Mock de servidores HTTP (providers) |
| `snap` | Snapshot testing |
| `test-case` | Testes parametrizados |
| `serial_test` | Testes que exigem exclusão mútua |
| `env-lock` | Serializa manipulação de variáveis de ambiente |
| `tempfile` | Diretórios temporários |
| `tokio-stream` | Streams em testes async |

## 3. Tiers de teste

### 3.1 Unitários inline
Em cada módulo (`#[cfg(test)] mod tests`). Ex.: hooks têm ~15 testes cobrindo fail-open, matchers, banner, plugin_root, etc.

### 3.2 Integração por crate (`tests/`)
Exemplos em `crates/goose/tests/`:

- `acp_*_test.rs` — ACP: bootstrap, provider, fork, secret cache invalidation, transport auth, custom requests, server.
- `mcp_integration_test.rs` — integração MCP.
- `compaction.rs` — compaction ponta a ponta.
- `providers.rs`, `tetrate_streaming.rs`, `litellm_default_host.rs`.
- `git_command_security.rs`, `schedule_tool_security.rs`, `permission_persistence.rs`.
- `subprocess_cleanup.rs`, `session_id_propagation_test.rs`, `state_machine_api.rs`.

### 3.3 Testes de ciclo de vida da máquina de estados
`crates/goose/src/agents/state_machine/tests/`:

`pipeline.rs`, `agent_reply.rs`, `tool_lifecycle.rs`, `compaction_lifecycle.rs`, `hooks_lifecycle.rs`, `provider_lifecycle.rs`, `recipe_scheduling_lifecycle.rs`, `prompt_skill_lifecycle.rs`, `steering_lifecycle.rs`, **`reconstruction_isolation_lifecycle.rs`**. Com fixtures próprias: `dummy_api.rs`, `calculator_extension.rs`.

O nome `reconstruction_isolation_lifecycle` é sintomático: testa que reconstruir o pipeline a partir da conversa persistida **isola** e reproduz o comportamento — exatamente o invariante que o `set_message_meta` sustenta.

### 3.4 Cenários end-to-end (`goose-cli/src/scenario_tests/`)
`scenario_runner.rs`, `mock_client.rs`, `provider_configs.rs`, `message_generator.rs`, com `recordings/` e `test_data/`. Testa o CLI de ponta a ponta com providers mockados.

## 4. Gravação e replay de MCP (`goose-test`)

Um diferencial: o goose grava interações MCP reais e as reproduz nos testes.

- `goose-test/src/bin/capture.rs` — captura.
- `goose-test/src/mcp/stdio/record.rs` — grava comandos e streams (STDIN/STDOUT/STDERR).
- `goose-test/src/mcp/stdio/playback.rs` — reproduz.

Os artefatos vivem em `crates/goose/tests/mcp_replays/`, com nomes que embutem a origem (ex.: `npx-y@modelcontextprotocol_server-everything@...`, `uvrun--withfastmcp==...fastmcpruntests_fastmcp_test_server.py`) e um `.results.json` ao lado. `just record-mcp-tests` regrava.

`goose-test-support` fornece fixtures: `mcp.rs`, `otel.rs`, `session.rs` (com `TEST_SESSION_ID`, `TEST_MODEL`, e `EnforceSessionId` que valida a propagação do header `agent-session-id`).

## 5. Self-testing

O repositório tem `goose-self-test.yaml` (25 KB) — uma recipe em que o goose **testa a si mesmo** usando suas próprias tools:

> "A comprehensive meta-testing recipe where goose tests its own capabilities using its own tools - true first-person integration testing."

Fases: operações de arquivo, shell, providers específicos (Azure AI Foundry, Databricks), análise de código, extensões, delegação, preservação de thinking multi-turn, Z.AI coding plan, ACP effort, observabilidade GDK, error boundaries, geração de relatório. Parâmetros: `test_phases`, `test_depth`, `workspace_dir`, `parallel_tests`, `cleanup_after`.

O `AGENTS.md` instrui: *"When adding features, update goose-self-test.yaml, rebuild, then run `goose run --recipe goose-self-test.yaml` to validate."*

## 6. CI e automação

Workflows relevantes (`.github/workflows/`):

| Workflow | Papel |
|---|---|
| `ci.yml` | CI principal |
| `cargo-deny.yml` | Auditoria de licenças/vulnerabilidades |
| `cargo-machete.yml` | Dependências não usadas |
| `mcp-conformance.yml` | Conformidade MCP |
| `model-toolcall-conformance.yml` | Conformidade de tool calling por modelo |
| `quarantine.yml` | Isolamento de testes instáveis |
| `pr-smoke-test.yml` | Smoke test de PR |
| `python-sdk-wheels.yml` / `maven-sdk.yml` | Empacotamento GDK (Python/Kotlin) |
| `gdk-release*.yml` | Release do GDK |
| `bundle-macos/windows`, `publish-docker/npm` | Distribuição |
| `goose-self-test.yaml` (via recipe) | Meta-teste |
| `goose-issue-solver.yml`, `goose-pr-reviewer.yml`, `code-review.yml` | O goose cuidando do próprio repo (dogfooding) |

## 7. Regras de qualidade (do `AGENTS.md`)

- Testes preferencialmente em `tests/` (`crates/goose/tests/`).
- `cargo fmt` obrigatório; `cargo clippy --all-targets -- -D warnings` obrigatório.
- Usar `anyhow::Result` para erros de aplicação.
- Evitar comentários que repetem o código; comentar apenas "por quê".
- Evitar `Option` desnecessário — "the compiler will enforce".
- Booleans default `false`, não `Option<bool>`.
- Evitar contexto de erro redundante e código defensivo em excesso ("trust Rust's type system").
- Nunca sobrescrever um binário em execução (SIGKILL no macOS): unlink/rename atômico.

## 8. Lições

1. **~4.000 testes** para um runtime de agente: confiança proporcional ao risco.
2. **Replay de MCP** permite testes determinísticos sem servidores externos.
3. **Testes de ciclo de vida** da máquina de estados, incluindo **isolamento de reconstrução**.
4. **Cenários E2E do CLI** com providers mockados.
5. **Self-testing** como meta-recipe — o agente valida a si mesmo.
6. **Conformidade por modelo** (`model-toolcall-conformance`) — reconhece que providers se comportam de forma diferente.
7. **Dependências de teste tratadas como cidadãs de primeira classe** no workspace (`wiremock`, `snap`, `test-case`, `serial_test`, `env-lock`).
