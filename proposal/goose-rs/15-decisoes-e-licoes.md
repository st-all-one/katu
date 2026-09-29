# 15 — Decisões e lições

Síntese do dossiê: as decisões de engenharia que definem o goose e o que delas aproveitar em `katu`.

## 1. As grandes decisões arquiteturais

| # | Decisão | Evidência | Consequência |
|---|---------|-----------|--------------|
| 1 | **Agente = stream de eventos** | `reply() -> BoxStream<Result<AgentEvent>>` | UI/CLI/ACP/HTTP compartilham lógica |
| 2 | **Máquina de estados plugável** | crate `goose-agent`, `Step = Operation\|Inference` | Comportamento extensível por composição |
| 3 | **Estado persistido a cada passo** | `machine.run` recarrega a sessão | Retomada, auditoria, testabilidade |
| 4 | **Contrato único de provider** | `Provider::stream` | ~28 impls nativas + 48 declarativos |
| 5 | **Providers declarativos em JSON** | `declarative/definitions/*.json` (48) | Custo marginal quase zero |
| 6 | **Uma abstração MCP, quatro origens** | `McpClientTrait` + `ExtensionConfig` | stdio/builtin/platform/HTTP uniformes |
| 7 | **SQLite + `BEGIN IMMEDIATE`** | `session_manager.rs` | Estado local transacional |
| 8 | **Compaction por visibilidade, não exclusão** | `agent_visible: false` | Transcript preservado |
| 9 | **Contrato de protocolo em crate próprio** | `goose-sdk-types` | Schema tipado p/ desktop e terceiros |
| 10 | **GDK publicado vs. app interna** | `release-plz.toml` | Fronteira de produto clara |
| 11 | **P2P/protocolos isolados por feature** | `roaming` + trait `AcpStreamServer` | Core leve |
| 12 | **Hooks com política de falha explícita** | `HookDecision` + `policy_evaluated` | Segurança auditável |
| 13 | **Testes de replay MCP + self-test** | `goose-test`, `goose-self-test.yaml` | Confiança sem serviços externos |

## 2. O que copiar (com convicção)

### 2.1 Interface de agente como stream de eventos tipado
Desacopla superfícies. Um único `AgentEvent` alimenta CLI, desktop, ACP e HTTP. **Vale para qualquer runtime de agente.**

### 2.2 Pipeline de operações (`Operation`/`Inference`) em vez de loop monolítico
- Primeira operação aplicável vence → sem ambiguidade.
- `set_message_meta` garante **determinismo reconstruível**.
- Cada operação é testável isoladamente.

Esse é o antídoto contra o "if gigante" que o próprio goose teve (`agent.rs`, 6.164 linhas) e está eliminando.

### 2.3 Persistência a cada passo com ordenação monotônica
`created = message.created.max(latest)` garante que nada seja inserido antes do que já existe — o mesmo espírito da invariante append-only do prefixo.

### 2.4 Providers declarativos
Se o dialeto é OpenAI/Anthropic/Ollama, é um JSON. Reduz N implementações a N configurações.

### 2.5 Unificação MCP
Uma trait, quatro origens. Extensões in-process para o que precisa do agente; subprocesso para o resto; HTTP para remoto.

### 2.6 Compactação que esconde, não apaga
`agent_visible: false` + resumo; `user_visible` intacto. Preserva auditoria e permite recomputar.

### 2.7 Fronteira GDK/app
Publicar o núcleo reutilizável (`goose-agent`, tipos, providers) e tratar a aplicação como descartável.

### 2.8 Verificação proporcional ao risco
Replay MCP, testes de ciclo de vida, isolamento de reconstrução, conformidade por modelo, self-test.

## 3. O que replicar com cuidado

| Aspecto | Risco | Mitigação sugerida |
|---|---|---|
| **`Agent` gigante** | `agent.rs` de 6k linhas concentra política | Manter operações pequenas e a máquina desacoplada |
| **Lifetimes `StateMachine<'a>`** | Operações emprestam campos do `Agent`; difícil destacar | Preferir estado próprio/clonável nas operações |
| **Timestamps como ordenação** | Empates de segundo resolvidos por `id`; frágil sob escrita concorrente | Considerar um `seq` monotônico explícito |
| **Metadata polimórfica** | `MessageMetadata` cresce com flags | Tipar por operação (`OperationNotes`) e evitar booleanos soltos |
| **Dois caminhos de loop** | Paridade legado/state-machine é custosa | Concluir a migração e remover o legado |
| **Duas linguagens no produto** | Rust + Electron/TS | Contrato único via ACP/GDK; nada de código gerado duplicado |
| **Features combinatoriais** | `rustls`/`native-tls` etc. | Validar CI por combinação; `compile_error!` já ajuda |
| **Compaction + concorrência** | Mensagens durante compaction | Teste de "preserva transcript concorrente" (já existe) |

## 4. goose vs. pi: dois pontos de vista sobre confiabilidade

Ambos os projetos convergem nos princípios, com implementações diferentes — útil para `katu`.

| Dimensão | **Pi** | **goose** |
|---|---|---|
| Foco | runtime **durável** com semântica de efeitos | runtime **produto** multi-superfície |
| Estado | três stores + invariante, transações atômicas com `seq` | SQLite (sessions/messages/usage_ledger), timestamps monotônicos |
| Loop | harness com intent→efeito→settlement | máquina de estados com operações e efeitos |
| Contexto | invariante append-only explícita (KV cache) | compaction por visibilidade + par tool |
| Efeitos externos | non-goal exactly-once, idempotência por operation id | confirmações/hooks com fail-open/block |
| Extensões | traits nativas (WASM depois) | MCP unificado (4 origens) |
| Protocolo | CBOR custom, versionado | ACP/MCP padrão + custom requests |
| Superfície | CLI/JSON/RPC | CLI + desktop + ACP + HTTP + P2P |

**Leitura:** o Pi é mais rigoroso quanto a **semântica de durabilidade**; o goose é mais rico em **produto e integração**. Um `katu` ambicioso pode pegar a disciplina de estado do Pi **e** a arquitetura de operações/MCP do goose.

## 5. Recomendações concretas para `katu`

1. **Copie a máquina de estados genérica** (`Step`/`Operation`/`Inference`/efeitos) — é o padrão mais valioso e portável.
2. **Adote stream de eventos** como contrato do agente.
3. **Persista a cada passo** e torne a recuperação o mesmo caminho da execução normal.
4. **Use um `seq` monotônico explícito** em vez de depender de timestamps.
5. **Separe contrato (`*-types`) de implementação** desde o início.
6. **MCP como camada de tools**, com uma trait única e origens plugáveis.
7. **Providers declarativos** sobre 2–3 dialetos antes de escrever SDKs nativos.
8. **Compactação por visibilidade**, com resumo estruturado e tolerante.
9. **Fronteira pública clara**: núcleo reutilizável publicável, app descartável.
10. **Verificação desde o início**: replay de protocolo, testes de ciclo de vida, isolamento de reconstrução.
11. **Hooks/política com decisão auditável** (`Allow`/`Deny` + causa), fail-open por padrão.
12. **Isolar dependências pesadas por feature** (P2P, inferência local, telemetria).

## 6. Anti-padrões observados (e como evitá-los)

- **Loop monolítico.** O goose está pagando o preço de migrar; comece com operações.
- **Estado implícito em metadata frouxa.** Prefira tipos por operação.
- **Ordenação por timestamp.** Use sequência monotônica.
- **Duplicar superfícies.** Um protocolo, muitos clientes.
- **Dependência pesada no core.** Isole via trait e feature.
- **Comentários que repetem o código.** O `AGENTS.md` do goose já proíbe; documente o "porquê".
- **Defensividade excessiva.** Confie no sistema de tipos.

## 7. Conclusão

O goose é um exemplo maduro de **agente como produto de plataforma**: um núcleo GDK publicável, uma máquina de estados extensível, um contrato de provider estreito com dezenas de implementações, MCP como espinha de tools, persistência SQLite a cada passo e muitas portas de entrada (CLI, desktop, ACP, HTTP, P2P).

Para `katu`, os dois maiores ativos a extrair são:

1. **A arquitetura de operações/efeitos reentrante** — um loop que se reconstrói da conversa persistida.
2. **A separação contrato/implementação/publicação** — tipos e agent-loop como biblioteca; aplicação como consumidor.

O restante — providers, MCP, compaction, hooks — são padrões bem documentados que este dossiê mapeia arquivo a arquivo para reconstrução fiel.
