# 10 — ACP e protocolos

O goose não é só um CLI/app: ele é um **nó de protocolo**. Fala MCP (extensões), ACP (editores e agentes), HTTP/WS, gateways de plataforma e P2P. Este documento cobre os protocolos de *agente*, complementando o MCP (doc 06).

## 1. ACP (Agent Client Protocol)

Base: crates `agent-client-protocol`, `agent-client-protocol-schema` (v1) e `agent-client-protocol-http`, além do proc-macro crate próprio `goose-acp-macros`.

O goose ocupa **dois papéis**:

### 1.1 goose como servidor ACP

`goose acp` (stdio) ou `goose serve` (HTTP/WS) expõem o agente para editores (JetBrains, Zed) e outros clientes.

`crates/goose/src/acp/` (11.728 linhas):

| Arquivo | LOC | Papel |
|---|---:|---|
| `provider.rs` | 4.729 | ACP como provider (agente externo → goose) |
| `server.rs` | 3.736 | Servidor ACP (métodos, sessões, prompts) |
| `server/providers.rs` | 1.281 | Lista/config de providers |
| `response_builder.rs` | 969 | Monta respostas ACP (estado de modelo, modos, opções) |
| `server/custom_dispatch.rs` | 921 | Despacho de métodos customizados |
| `handoff.rs` | 618 | Handoff de sessão |
| `mcp_app_proxy.rs` | 502 | Proxy MCP Apps |
| `fs.rs` | 475 | Acesso a arquivos exposto ao cliente |
| `transport/{mod,tls,auth}.rs` | ~460 | Transporte stdio/HTTP/WS, TLS, auth |

Métodos ACP cobertos (parcial): `Initialize`, `NewSession`, `LoadSession`, `ForkSession`, `CloseSession`, `DeleteSession`, `ListSessions`, `Prompt`, `Cancel`, `SetSessionMode`, `SetSessionConfigOption`, `RequestPermission`, `Authenticate`, além de `SessionNotification`/`SessionUpdate`.

O `response_builder.rs` monta metadados de sessão (`build_model_state`, `build_provider_options`, `build_session_info`, `session_meta`, `should_refresh_inventory_for_session_init`), permitindo que o cliente ACP exiba/altere modelo, provider, modo e thinking effort.

### 1.2 goose como cliente ACP (providers)

Via `acp/provider.rs` + os providers `*_acp.rs`, o goose delega a outros agentes (Claude Code, Codex, Copilot, Amp, **Pi**). O agente externo executa tools internamente; o goose repassa extensões como servidores MCP.

## 2. Métodos e notificações customizados

O goose estende o ACP com um namespace próprio, definido em **`goose-sdk-types`** (crate GDK):

### Custom requests (`custom_requests.rs`)

`AddSessionExtension`, `RemoveSessionExtension`, `GetTools`, `ReadResource`, `GooseToolCall`, `AppsList/Export/Import/Delete`, `UpdateWorkingDir`, `SetSessionSystemPrompt` (+`SessionSystemPromptMode`), `SteerSession`, `LiveVoiceAvailability/Start/Stop`, `DiagnosticsGet`, além de requests de recipe e schedule.

### Custom notifications (`custom_notifications.rs`)

`GooseSessionNotification` (+`GooseSessionUpdate`), `LiveVoiceInteractionEnded`, `ProviderDeviceCode`, `SessionUsageUpdate`, `MessageUsageUpdate`, `StatusMessageUpdate` (com enum `StatusMessage`).

Há ainda `custom_notification_schemas(...)` que gera os schemas (via `schemars`) para publicação.

**Isso é a materialização do "protocolo como contrato"**: a extensão ACP do goose é tipada em um crate separado, com schemas gerados, e o desktop e terceiros consomem os mesmos tipos.

## 3. `serve` — ACP sobre HTTP/WebSocket

`goose serve` (feature `acp-http`):

- `--host` (default `127.0.0.1`), `--port` (default `3284`), `--platform cli|desktop`.
- `--tls`, `--tls-cert-path`, `--tls-key-path` (usa `axum-server` + `rustls`/`rcgen`).
- Autenticação via `GOOSE_SERVER__SECRET_KEY`; `--dangerously-unauthenticated` desabilita.
- `--allowed-origin` (CORS, substitui os loopback defaults).
- `--with-builtin`, `--enable-scheduler`, `--roam`.

O transporte (`transport/tls.rs`, `transport/auth.rs`) e o `auth` refletem uma preocupação explícita com exposição de rede.

## 4. `gateway` — integração com plataformas externas

`crates/goose/src/gateway/` (4.368 linhas) permite dirigir o goose a partir de chats externos (ex.: Telegram).

```rust
pub trait Gateway: Send + Sync + 'static {
    fn gateway_type(&self) -> &str;
    async fn start(&self, handler: GatewayHandler, cancel: CancellationToken) -> Result<()>;
    async fn send_message(&self, user: &PlatformUser, message: OutgoingMessage) -> Result<()>;
    async fn validate_config(&self) -> Result<()>;
    fn info(&self) -> HashMap<String, String> { HashMap::new() }
}
```

Tipos: `GatewayHandler`, `GatewayManager`, `GatewayInstance`, `GatewayConfig`, `IncomingMessage`, `OutgoingMessage`, `PairingState`, `Attachment`, `PairedUserInfo`, `PlatformUser`.

Há um **fluxo de pairing** (`pairing.rs`, `PairingStore`, códigos pendentes) e allowlist de usuários (`saved_allowed_user_ids`). Hoje apenas `telegram` é instanciado em `create_gateway`.

## 5. `roaming` — P2P via iroh

Crate `goose-roaming` (2.197 linhas). Objetivo (do próprio doc):

> "lets a goose agent expose itself over the internet via iroh so that a remote ACP client ... can connect to and drive it through a relay, with no open ports."

Blocos:

| Tipo | Papel |
|---|---|
| `RoamingIdentity` | Chave ed25519 persistida; a metade pública é o endpoint id (self-certifying no QUIC-TLS) |
| `ConnectionCard` | String compartilhável (chave pública + relays + fingerprint); não expira e não concede nada |
| `TrustBook` | Allowlist mútua local + revogações; acesso só por aceitar a chave, **sem bearer token** |
| `RoamingNode` | Dono do endpoint iroh + router; hospeda agentes no ALPN `goose-acp/1`; disca agentes remotos |

Decisão de design explícita:

> "The crate deliberately knows nothing about goose's agent internals: hosting is driven through the `AcpStreamServer` trait, which the integration layer implements by calling goose's generic `acp::server::serve`. This keeps the heavy iroh dependency out of the `goose` core crate entirely."

Ou seja: **P2P é opcional e isolado** via feature `roaming`.

## 6. `execution` — múltiplos agentes por processo

`crates/goose/src/execution/`:

- `AgentManager` — cria/recupera agentes por sessão (`RuntimeContext`).
- `ActiveRunRegistry` — rastreia runs ativos (evita execuções concorrentes).

Isso sustenta o modelo ACP/serve onde um processo atende várias sessões.

## 7. `goose_apps` — MCP Apps / UI

Já visto no doc 06: recursos de UI expostos por extensões MCP, com metadados de CSP, permissões e janela, conectando servidores MCP a componentes do desktop.

## 8. Lições

1. **Um agente, muitos protocolos.** MCP, ACP, HTTP/WS, P2P, gateway — cada um isolado atrás de features/traits.
2. **Contrato ACP tipado em crate separado** (`goose-sdk-types`) com schemas gerados.
3. **P2P isolado do núcleo** via trait `AcpStreamServer` — dependência pesada fora do core.
4. **Segurança explícita**: auth por secret, TLS, CORS, pairing, allowlist, trust book.
5. **Mesmo motor, múltiplas sessões** via `AgentManager` + `ActiveRunRegistry`.
