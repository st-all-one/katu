# 15 — Riscos e Decisões (ADRs)

## Riscos técnicos

### R1 — Superfície de providers (~45) e catálogo de modelos
**Risco**: reimplementar todos os adapters e manter o catálogo atualizado é o maior custo contínuo.
**Mitigação**: adapter OpenAI-compatible parametrizado cobre a maioria; priorizar 3 APIs de wire distintas; gerar catálogo por `xtask` a partir das mesmas fontes upstream; aceitar paridade parcial nas fases iniciais.

### R2 — Fidelidade do streaming normalizado
**Risco**: divergência de eventos (ordem, `contentIndex`, `partial`, erros dentro do stream) quebra consumidores e evals.
**Mitigação**: portar o `faux` provider + testes de eventos golden; tratar `partial` como não-snapshot; documentar; testes de intercalação de blocos.

### R3 — TUI: largura, ANSI, main/alt screen
**Risco**: é a parte com mais edge cases e difícil de testar.
**Mitigação**: `crossterm` + `unicode-width` + emulador headless + golden files; portar o `tui-plan.md` com testes de layout; tratar imagens/clipboard por último.

### R4 — Sistema de extensões
**Risco**: incompatibilidade com o ecossistema de extensões TS; perda de "self extensible".
**Mitigação**: traits nativos primeiro; WASM depois; manter RPC como escape hatch; documentar a quebra claramente; portar recursos declarativos (skills/templates/temas) cedo.

### R5 — Compatibilidade de formato de sessão
**Risco**: sessões do Pi original não abrirem ou perderem dados.
**Mitigação**: portar o parser com todos os campos, migrações v1→v3, testes de round-trip e compatibilidade bidirecional.

### R6 — Strings de prompt e comportamento do modelo
**Risco**: pequenas diferenças no system prompt/guidelines mudam comportamento e quebram evals.
**Mitigação**: copiar strings literalmente; testes de snapshot do prompt montado.

### R7 — `chord`/durável/remoto
**Risco**: complexidade alta e valor duvidoso para o MVP.
**Mitigação**: adiar; manter só se houver caso de uso concreto; reduzir a um serviço leve.

### R8 — Paridade de concorrência e cancelamento
**Risco**: semântica de `AbortSignal`, abort de tools, `waitForIdle`, `agent_settled` diferir.
**Mitigação**: `CancellationToken` + testes de fluxo de eventos (abort/erro/retry).

## Decisões arquiteturais (ADRs)

### ADR-001 — Crate por pacote
**Status**: aceita.
**Contexto**: o monorepo TS já tem fronteiras limpas.
**Decisão**: um crate por pacote npm, mantendo nomes/relações.
**Consequência**: isolamento, builds incrementais, feature flags por camada.

### ADR-002 — `pi-ai` sem dependência de I/O no núcleo
**Status**: aceita.
**Contexto**: o `ai` original é side-effect-free no core.
**Decisão**: tipos/registry puros; providers atrás de feature flags; `reqwest` só nos adapters.
**Consequência**: compila para WASM/embarcado se necessário; testes rápidos.

### ADR-003 — `AgentMessage` como enum fechado + `Custom`
**Status**: aceita.
**Contexto**: TS usa declaration merging; Rust não tem union extensível.
**Decisão**: enum `#[non_exhaustive]` com variantes conhecidas + `Custom { custom_type, ... }`.
**Consequência**: hosts usam `Custom`; sem reflexão; mais seguro.

### ADR-004 — Streaming com snapshots em vez de `partial` mutável
**Status**: aceita (com ressalva).
**Contexto**: o original muta a mensagem compartilhada.
**Decisão**: emitir eventos com `contentIndex` e um `partial` clonado/imutável por evento.
**Consequência**: mais alocações, porém seguro; o contrato do original já proíbe reter `partial`.

### ADR-005 — `crossterm` + diff próprio (não `ratatui`)
**Status**: aceita.
**Contexto**: semântica específica de main/alt screen, scrollback, CSI 2026 e layout de stack.
**Decisão**: `crossterm` para I/O, implementar rendering diferencial e layout próprios.
**Consequência**: mais trabalho de TUI, controle total.

### ADR-006 — Persistência JSONL compatível byte-a-byte (campos)
**Status**: aceita.
**Contexto**: interoperabilidade com o Pi original.
**Decisão**: preservar nomes de campos e estrutura de árvore; manter migrações.
**Consequência**: serde com rename explícito; testes de compatibilidade.

### ADR-007 — Extensões: traits nativos no MVP, WASM depois
**Status**: aceita.
**Contexto**: sem Node, não há loader TS trivial.
**Decisão**: traits nativos; WASM (`wasmtime`) numa fase seguinte; RPC para outras linguagens.
**Consequência**: quebra de compatibilidade com extensões TS; documentada.

### ADR-008 — `chord` adiado
**Status**: aceita.
**Contexto**: complexidade alta, fora do caminho crítico do CLI.
**Decisão**: não portar no MVP; reavaliar na fase de remoto/durável.
**Consequência**: sem facet hosts/bundler/delta replication inicialmente.

### ADR-009 — Evals permanecem em Node
**Status**: aceita.
**Contexto**: infra madura (`vitest-evals`), roda contra o binário.
**Decisão**: não portar; executar contra `pi-rs` via CLI/RPC.
**Consequência**: mantém uma dependência de Node só para evals/CI.

### ADR-010 — `reqwest` + adapters próprios (não SDKs Rust de provider)
**Status**: aceita.
**Contexto**: a normalização de streaming é o valor do Pi; SDKs oficiais variam.
**Decisão**: implementar adapters sobre HTTP/SSE/WS, reusando o modelo de tipos.
**Consequência**: controle da normalização; mais código; AWS SDK só para Bedrock se vantajoso.

### ADR-011 — `rusqlite` bundled
**Status**: aceita.
**Contexto**: `node:sqlite` no original; queremos binário autossuficiente.
**Decisão**: `rusqlite` com feature `bundled`.
**Consequência**: compila SQLite junto; sem dependência de sistema; build mais lento.

### ADR-012 — Catálogo de modelos via `xtask`
**Status**: aceita.
**Contexto**: `models.generated.ts` é gerado no build do `ai`.
**Decisão**: `xtask generate-models` produz `catalog/generated.rs`/`.json`; nunca editar à mão.
**Consequência**: requer acesso upstream no build; suportar modo offline com dados versionados.

## Questões abertas

1. **Escopo de `chord`/remoto**: há necessidade real de multi-processo no curto prazo?
2. **Compatibilidade de extensões TS**: aceitar a quebra ou investir em `deno_core`?
3. **`pi-durable`/Pico**: manter o runtime durável ou simplificar para sessão JSONL?
4. **Nome/versionamento**: `pi-rs` mantém os caminhos `~/.pi/...` para interoperar? (recomendado sim)
5. **Providers**: quais são obrigatórios no MVP (OpenAI/Anthropic/Google provavelmente)?
6. **Distribuição**: manter os mesmos canais (npm wrapper? binário direto? `rustup`?).

## Licença e atribuição

O Pi é MIT (Copyright 2025 Mario Zechner). A reimplementação deve:
- Incluir o texto da licença MIT e a atribuição de copyright original.
- Notar claramente que `pi-rs` é uma reimplementação independente.
- Preservar avisos de licença de dependências incorporadas (vendor JS no export HTML, etc.).
