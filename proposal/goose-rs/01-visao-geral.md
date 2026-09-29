# 01 — Visão geral do goose

## 1. O que é

goose é descrito pelos próprios mantenedores como:

> "a general-purpose AI agent that runs on your machine. Not just for code — use it for research, writing, automation, data analysis, or anything you need to get done."

É um agente **multi-superfície**:

- **Desktop app** nativa (macOS/Linux/Windows) — Electron + React.
- **CLI completa** (`goose-cli`) para fluxos de terminal.
- **API/SDK** para embutir em outros produtos (GDK, ACP, HTTP).

O README destaca três pilares:

1. **Local-first / native** — "runs on your machine", "built in Rust for performance and portability".
2. **Multi-provider** — "15+ providers" (Anthropic, OpenAI, Google, Ollama, OpenRouter, Azure, Bedrock, …), incluindo uso de assinaturas existentes (Claude/ChatGPT/Gemini) via ACP.
3. **Extensível** — "70+ extensions via the Model Context Protocol (MCP)".

## 2. Componentização (modelo mental oficial)

A documentação de arquitetura (`documentation/docs/goose-architecture/goose-architecture.md`) define **três componentes**:

| Componente | Papel |
|---|---|
| **Interface** | Desktop ou CLI. Coleta input, exibe output. |
| **Agent** | Roda a lógica central e gerencia o loop interativo. |
| **Extensions** | Fornecem tools/habilidades (arquivos, comandos, web, memória…). |

Numa sessão típica: a interface instancia um **Agent**, que se conecta a **uma ou mais extensões** simultaneamente. A interface pode criar **múltiplos agentes** para tarefas concorrentes.

## 3. O loop interativo (conforme documentado)

1. **Human Request** — o usuário dá a instrução.
2. **Provider Chat** — goose envia a requisição + lista de tools ao provider; o modelo pode emitir um *tool call*.
3. **Model Extension Call** — o modelo **pede**, o goose **executa** a tool e coleta o resultado.
4. **Response to Model** — o resultado volta ao modelo; repete se necessário.
5. **Context Revision** — goose remove/sumariza informação antiga (gestão de tokens).
6. **Model Response** — resposta final ao usuário; o loop recomeça.

## 4. Tratamento de erros como princípio

> "As opposed to allowing an error to break the flow, goose captures and handles traditional errors along with execution errors. Errors such as invalid JSON, missing tools, etc. are sent back to the model as tool responses giving the LLM the information it needs to resolve the error and continue."

Ou seja: **erro de tool vira conteúdo de conversa**, não exceção fatal. Existe documentação dedicada (`goose-architecture/error-handling.md`).

## 5. Superfícies de integração (além da UI)

O goose fala **muitos protocolos**, o que é central para sua estratégia de produto:

- **MCP** (Model Context Protocol) — como consumidor de extensões e como fornecedor de servidores embutidos (`goose mcp`).
- **ACP** (Agent Client Protocol) — goose como **servidor ACP** para editores (JetBrains, Zed) e como **cliente** (delega a agentes externos como Claude Code/Codex).
- **HTTP/WebSocket** — `goose serve` expõe ACP sobre rede.
- **Roaming** — compartilhamento P2P entre dispositivos via `iroh`.
- **Gateway** — integração com plataformas externas (ex.: Telegram).

## 6. Distribuição e governança

- Faz parte da **Agentic AI Foundation (AAIF)** na Linux Foundation.
- Licença **Apache-2.0**.
- Publicação: binários (desktop + CLI) e crates do **GDK** no crates.io.
- Existe suporte a **Custom Distributions** (`CUSTOM_DISTROS.md`): construir um goose próprio com providers, extensões e branding pré-configurados.

## 7. Onde está a complexidade real

Ao ler o repositório, fica claro que a complexidade **não** está no "chamar o LLM" e sim em:

1. **Persistência e retomada** de conversas (SQLite, 5.025 linhas só em `session_manager.rs`).
2. **Gestão de contexto** (compaction, tool-pair summarization, limites por modelo).
3. **Integração MCP** (clientes stdio/HTTP, OAuth, notificações, elicitation).
4. **Multi-provider** (48 providers declarativos + ~28 implementações nativas + OAuth).
5. **Permissões e segurança** (modos, inspectors, scanning de conteúdo).
6. **Múltiplas superfícies** (CLI, desktop, ACP, HTTP, gateway, P2P).

## 8. Fatos estruturais que orientam o resto do dossiê

- O código é um **monorepo Cargo**; a maior parte vive no crate `goose`.
- O agente está em **transição**: um loop legado (`agents/agent.rs`) coexiste com uma **máquina de estados** (`agents/state_machine/` + crate `goose-agent`), controlada por `GOOSE_STATE_MACHINE=1`.
- O estado do agente é **sempre persistido**: cada passo recarrega a sessão do storage.
- O "contrato" com providers é um trait único (`Provider::stream`) que devolve um **stream de mensagens parciais + tool calls completos**.
- As extensões são **clientes MCP** unificados atrás de `McpClientTrait`.

Esses cinco pontos são desenvolvidos nos documentos seguintes.
