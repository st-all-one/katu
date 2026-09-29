# 06 — Extensões e MCP

## 1. As extensões são o mecanismo de "tool"

A documentação é explícita: extensões expõem funcionalidade ao agente através de **tools**. Quase tudo que o goose "faz" é uma tool — arquivos, shell, web, memória, análise de código, etc. A interoperabilidade é via **Model Context Protocol (MCP)**.

O ponto crucial de implementação: o goose trata **quatro origens diferentes** de tools através de **uma única abstração** (`McpClientTrait`).

## 2. `McpClientTrait` — a abstração unificada

`agents/mcp_client.rs`:

```rust
#[async_trait]
pub trait McpClientTrait: Send + Sync {
    async fn list_tools(&self, session_id: &str, next_cursor: Option<String>, cancel: CancellationToken)
        -> Result<ListToolsResult, Error>;
    async fn call_tool(&self, ctx: &ToolCallContext, name: &str, arguments: Option<JsonObject>, cancel: CancellationToken)
        -> Result<CallToolResult, Error>;
    fn get_info(&self) -> Option<&InitializeResult>;

    fn get_instructions(&self) -> Option<String> { /* default lê get_info() */ }
    async fn list_resources(...) -> Result<ListResourcesResult, Error> { Err(Error::TransportClosed) }
    async fn read_resource(...) -> Result<ReadResourceResult, Error> { Err(Error::TransportClosed) }
    async fn list_prompts(...) -> Result<ListPromptsResult, Error> { Err(Error::TransportClosed) }
    async fn get_prompt(...) -> Result<GetPromptResult, Error> { Err(Error::TransportClosed) }
    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> { mpsc::channel(1).1 }
    async fn get_moim(&self, _session_id: &str) -> Option<String> { None }
    async fn update_working_dir(&self, _new_dir: PathBuf) -> Result<(), Error> { Ok(()) }
}
```

Detalhes importantes:

- `get_instructions()` é **sobrescrevível** para extensões de plataforma que computam instruções dinamicamente (ex.: skills recém-descobertas).
- `get_moim()` permite que uma extensão injete conteúdo no **MOIM** (Model-Observed Internal Memory) do turno.
- `update_working_dir()` deixa a extensão reagir a mudanças de diretório.
- O default de recursos/prompts devolve `TransportClosed` — implementação opcional.

Componentes concretos:

- `GooseClient` — cliente MCP base com handlers de notificação.
- `McpClient` — cliente concreto de stdio/HTTP.
- `GooseMcpHostInfo`, `GooseMcpClientCapabilities`, `ElicitationHandler`.
- `ActiveToolCallGuard` — guarda RAII que remove a tool call ativa no `Drop` (rastreio de tools em execução por sessão).

## 3. As quatro origens de extensão

`agents/extension.rs`:

```rust
#[serde(tag = "type")]
pub enum ExtensionConfig {
    #[serde(rename = "stdio")]
    Stdio { name, description, cmd, args, envs, env_keys, timeout, cwd, bundled, available_tools },

    #[serde(rename = "builtin")]
    Builtin { name, description, display_name, timeout, bundled, available_tools },

    #[serde(rename = "platform")]
    Platform { name, description, display_name, bundled, available_tools },

    #[serde(rename = "streamable_http")]
    StreamableHttp {
        name, description, uri, envs, env_keys, headers, timeout,
        socket,                 // HTTP sobre Unix domain socket
        client_id,              // OAuth client ID pré-registrado
        client_secret_key,      // segredo resolvido de env/secret store
        scopes, bundled, available_tools,
    },
}
```

| Tipo | O que é | Onde roda |
|---|---|---|
| **stdio** | Processo-launcher (`npx`, `python`, `uvx`…) falando MCP por stdin/stdout | subprocesso |
| **builtin** | Servidor MCP embutido no binário `goose` | subprocesso `goose mcp <name>` |
| **platform** | Extensão **in-process** com acesso direto ao agente | processo do agente |
| **streamable_http** | Servidor MCP remoto sobre HTTP/WS | remoto |

Cada config aceita `available_tools` — uma **allowlist** de tools expostas.

### Tratamento de segredos

`Envs` + `env_keys`:

- `envs` — valores literais;
- `env_keys` — **nomes de chaves** resolvidos de um secret store (keyring), nunca embutidos.

Há checagem de chaves proibidas (`is_disallowed_key`), garantindo que segredos não vazem em texto claro.

## 4. Servidores MCP embutidos (`goose-mcp`)

O binário `goose mcp <server>` inicia um servidor MCP por stdio. `McpCommand`:

| Servidor | Função |
|---|---|
| `autovisualiser` | Visualização de dados (templates) |
| `computercontroller` | Automação: docx, pdf, xlsx, web scraping |
| `memory` | Memória persistente |
| `tutorial` | Tutoriais interativos (tutoriais embutidos) |

Há também um subcomando oculto `peekaboo` (`crates/goose-mcp/src/peekaboo/`).

## 5. Extensões de plataforma (`agents/platform_extensions/`)

Rodam **in-process** e podem acessar o agente diretamente. Definidas em `PLATFORM_EXTENSIONS` (um `Lazy<HashMap<&str, PlatformExtensionDef>>`). Exemplos:

| Extensão | Função |
|---|---|
| `analyze` | Análise de código com tree-sitter (call graphs, símbolos) |
| `todo` | Lista de tarefas do agente |
| `summarize` | Sumarização |
| `developer` | Edição/escrita de arquivos (a "developer extension") |
| `ext_manager` | Gerencia extensões (instalar/remover/descobrir) |
| `orchestrator` | Orquestração de subagentes |
| `scheduler` | Agendamento de recipes |
| `summon` | Invocação de subagentes |
| `tom` | "Theory of mind" / contexto do usuário |
| `apps` | MCP Apps (UI) |
| `code_execution` | Code mode |
| `chatrecall` | Busca no histórico de chats |

Cada `PlatformExtensionDef` traz `name`, `display_name`, `description`, `default_enabled`, `unprefixed_tools`, `hidden`, e um `client_factory: |ctx| -> Option<Box<dyn McpClientTrait>>`.

## 6. `ExtensionManager` — o ciclo de vida

`agents/extension_manager/mod.rs` (2.782 linhas) é o subsistema que:

- Instancia clientes por `ExtensionConfig` (stdio → spawn de subprocesso; builtin → spawn de `goose mcp`; platform → in-process; http → cliente remoto).
- Faz **initialize** MCP, descoberta de tools/recursos/prompts.
- Mantém nomes de tools prefixados por extensão (evitando colisões).
- Gerencia notificações de servidor (subscriptions).
- Suporta Docker (`--container`): inicia a extensão dentro de um container.
- Trata OAuth para HTTP.
- Faz **malware check** (`extension_malware_check.rs`, 35 KB) sobre extensões.
- Valida extensões (`validate_extensions.rs`).

Transportes concretos:

- `extension_manager/stdio.rs`
- `extension_manager/streamable_http.rs` (39 KB)
- `builtin.rs`

## 7. Ferramentas de gestão de extensões

Existem tools internas para o próprio agente gerenciar extensões em tempo de execução (`ext_manager.rs`):

- `MANAGE_EXTENSIONS_TOOL_NAME` (+ `_COMPLETE`)
- `SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME`

E o CLI tem `goose validate-extensions` e `goose mcp-probe` (um script de steps para testar servidores MCP).

## 8. MCP Apps (`goose_apps/`)

O goose suporta **MCP Apps / MCP-UI**: extensões podem expor recursos de UI. Tipos:

- `GooseApp`, `WindowProps`
- `McpAppCache`, `mark_deletable_apps`
- `McpAppResource`, `ResourceMetadata`, `CspMetadata`, `PermissionsMetadata`, `UiMetadata`

Isso conecta o servidor MCP a componentes de UI no desktop.

## 9. Lições

1. **Uma abstração, quatro origens.** `McpClientTrait` unifica stdio, builtin, in-process e HTTP.
2. **Segredos por referência (`env_keys`), nunca inline.**
3. **`available_tools`** como allowlist por extensão.
4. **Extensões in-process (platform)** para capacidades que precisam do agente.
5. **Ciclo de vida com Docker, OAuth, malware check e notificações** — a extensão é tratada como código não confiável.
6. **`get_moim`** permite que extensões participem do contexto do turno.
7. **Self-management**: o agente pode instalar/descobrir extensões via tools.
