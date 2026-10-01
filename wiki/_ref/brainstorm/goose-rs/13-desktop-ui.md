# 13 — Desktop UI

## 1. Stack

O app desktop (`ui/desktop/`, ~89.240 linhas TS/TSX) é:

- **Electron Forge** (build/packaging) + **Vite** + **React**.
- Node `^24.10.0`, pnpm `>=10.30.0`.
- UI: **Radix UI** (componentes acessíveis), tema próprio, i18n com **formatjs** (en/es/ja).
- Protocolo: **`@agentclientprotocol/sdk`** e o pacote workspace **`@aaif/goose-acp-client`**.
- MCP Apps/UI: **`@mcp-ui/client`** e **`@modelcontextprotocol/ext-apps`**.

Do README:

> "This is an Electron Forge app using Vite and React. The desktop app launches the bundled `goose` CLI binary and talks to its ACP server."

Ou seja: **o desktop não reimplementa o agente**. Ele é um **cliente ACP** do binário `goose`.

## 2. Modelo de processo

```
┌────────────────────────── Electron ──────────────────────────┐
│  main.ts  (processo principal, 3.205 linhas)                  │
│    • inicia/gerencia o binário goose (ACL/ACP server)         │
│    • proxy, certificados, updates, downloads                  │
│    • preload.ts → ponte segura ao renderer                    │
│                                                               │
│  renderer.tsx  (React app)                                    │
│    • usa @aaif/goose-acp-client para falar ACP                │
│    • componentes em src/components (79 entradas)              │
└───────────────────────────────────────────────────────────────┘
                              │ ACP (stdio/HTTP)
                    ┌─────────▼─────────┐
                    │  binário goose    │
                    └───────────────────┘
```

Nota: o `AGENTS.md` proíbe explicitamente importar código de cliente OpenAPI gerado em `ui/desktop/src/api`; a comunicação se dá pelos **tipos ACP**. É uma regra de arquitetura deliberada.

## 3. Arquivos de infraestrutura (processo principal)

| Arquivo | Papel |
|---|---|
| `main.ts` | Entry do Electron, lifecycle, integração com o binário |
| `preload.ts` | Bridge segura ao renderer |
| `renderer.tsx` | Bootstrap do React |
| `gooseServe.ts` (606) | Inicia/gerencia o `goose serve` (ACP sobre HTTP) |
| `gooseServeLeaseRegistry.ts` | "Lease" de processo serve (evita duplicação) |
| `proxy.ts` | Proxy de rede |
| `remoteBackends.ts` | Backends remotos |
| `backendCertificateVerifier.ts` | Verificação de certificado TLS |
| `desktopFileAccess.ts` | Acesso a arquivos do desktop |
| `sessionLinks.ts` / `sessions.ts` | Deep links e sessões |
| `startupDiagnostics.ts` | Diagnóstico de inicialização |
| `updates.ts` | Auto-update |
| `loginShellPath.ts` | Resolução de PATH do shell de login |
| `liveVoice/` | Voz em tempo real |
| `recipe/` | Suporte a recipes/deeplinks |

## 4. Estrutura de UI

`src/components/` (79 entradas), entre elas:

- Chat: `BaseChat.tsx`, `ChatInput.tsx`, `ChatInputCard.tsx`, `GooseMessage.tsx`, `MarkdownContent.tsx`, `ChatSessionsContainer.tsx`.
- Extensões: `ExtensionInstallModal.tsx`, `extensions/`, `MentionPopover`.
- MCP Apps: `McpApps/`, `apps/`.
- Elicitation: `ElicitationRequest.tsx`.
- Contexto: `context_management/`, `ConfigContext.tsx`.
- Sistema: `ErrorBoundary.tsx`, `Layout/`, `GooseSidebar/`, `toasts.tsx`.
- Voz: `LiveVoiceButton.tsx`.

Contextos (`src/contexts/`), hooks (`src/hooks/`), tema (`src/theme/`), tipos (`src/types/` — tipos locais permitidos pelo `AGENTS.md`).

## 5. Integração ACP em detalhe

O desktop usa os **tipos de protocolo** do goose:

- `src/acp/` — integração direta.
- `@aaif/goose-acp-client` (em `ui/goose-acp-client/`, também há `ui/goose-acp/`) — cliente ACP empacotado.
- Métodos customizados do goose (`goose.*`) dão à UI recursos como: gerenciar extensões por sessão, listar tools, executar tool calls, recursos/prompts, MCP Apps, steer, voz, system prompt, working dir, diagnóstico, usage.

Isso fecha o ciclo: os **tipos custom** definidos em `goose-sdk-types` (Rust) são o contrato consumido pelo desktop via ACP.

## 6. Qualidade e testes

- **Typecheck**: `tsc --noEmit`.
- **Lint**: ESLint + Prettier + validação i18n.
- **Testes unitários**: Vitest (`pnpm test`).
- **Testes E2E**: Playwright (`test-e2e`, com ui/debug/report).
- **Testes de integração**: Vitest com config separada (inclui testes de providers).
- Vários módulos têm testes co-localizados (`*.test.ts(x)`): `gooseServe.test.ts`, `proxy.test.ts`, `remoteBackends.test.ts`, `backendCertificateVerifier.test.ts`, `desktopFileAccess.test.ts`, etc.

## 7. Distribuição

- Electron Forge `make`/`package`, com bundling de binários por plataforma (`scripts/prepare-platform-binaries.js`).
- `forge.config.ts` + `.deb`/`.rpm` desktop entries + `entitlements.plist` (macOS).
- `app-update.yml` / `updates.ts` para auto-update.

## 8. Lições

1. **UI como cliente de protocolo.** O desktop não duplica o agente; consome ACP.
2. **Contrato único (tipos ACP/GDK)** entre Rust e TypeScript — proibido importar clientes OpenAPI gerados.
3. **`goose serve` + lease registry** para gerenciar o processo do agente com segurança.
4. **i18n de primeira classe** (formatjs; en/es/ja) com validação no CI.
5. **MCP Apps/UI** integradas para extensões com interface.
6. **Testes em camadas** (unit Vitest, E2E Playwright, integração) — espelhando a estratégia do backend Rust.
