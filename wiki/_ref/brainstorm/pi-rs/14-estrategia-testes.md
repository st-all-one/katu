# 14 — Estratégia de Testes

## 1. Cenário atual

| Pacote | Arquivos `*.test.ts` |
|---|---:|
| `ai` | 156 |
| `agent` | 78 |
| `coding-agent` | 295 |
| `tui` | 39 |
| outros | (chord, client, durable, protocol, server, sqlite, telemetry) |
| **Total principal** | **~568+** |

Infra:
- `vitest` (runner), `@vitest/coverage-v8`.
- `test.sh` roda testes em **home isolado**, ambiente limpo, sem API keys, `LANG=C`, `TZ=UTC`, `PI_NO_LOCAL_LLM=1`. Garante reprodutibilidade e evita dados de usuário.
- `coding-agent/test/suite/harness.ts` + **faux provider** para testes de agente sem rede.
- Testes com API keys reais são opt-in (via env) e pulados por padrão.
- `@xterm/headless` para validar output ANSI do TUI.
- `evals` (vitest-evals) para comportamento end-to-end com LLM real (host e docker).
- `mini-test.sh`, `pi-test.sh/.bat/.ps1` para smoke local.

## 2. Estratégia para o Rust

### 2.1 Paridade como requisito
Manter o Pi original como oráculo. Para cada subsistema, gerar **fixtures de entrada/saída** a partir do original e consumir no `pi-rs`:
- Catálogo de modelos (JSON).
- Sessões JSONL de exemplo (v1/v2/v3) e resultado esperado de `buildContext`.
- Seqüências de eventos do faux provider.
- Frames CBOR (bytes) e decodificação esperada.
- Outputs de tools (`read`/`edit`/`grep`) e diffs.

### 2.2 Tipos de teste
| Tipo | Ferramenta | Escopo |
|---|---|---|
| Unitário | `cargo test` | funções puras (parsing, edit-diff, cores, layout) |
| Propriedade | `proptest` | JSONL round-trip, framing fragmentado, ANSI truncation |
| Snapshot | `insta` | eventos, render de TUI, output de tools |
| Integração | `tokio::test` | loop do agente com faux provider |
| Conformance | harness próprio | session storage, storage (durable), protocol |
| Golden terminal | `vt100`/`avt` | frames TUI |
| E2E | binário `pi` | print/json/rpc com faux provider |
| Evals | manter Node | comportamento real com LLM |

### 2.3 Conformance suites
Portar os runners de conformance existentes:
- **Session repo/storage conformance** (`pi-agent/test/harness/memory-conformance`): um conjunto de casos que qualquer backend de sessão deve passar. Implementar como trait + macro de teste, com adaptadores para memória/JSONL/SQLite.
- **Storage conformance** (`pi-durable/testing`): casos registro-nível.
- **Protocol codec**: fragmentação/coalescência de chunks deve produzir a mesma sequência de mensagens.

### 2.4 Faux provider
Portar primeiro o `faux` provider com scripts determinísticos de resposta (texto, thinking, tool calls, erro, abort). É a base de todos os testes de agente/harness sem rede.

### 2.5 Testes de UI
- Emulador headless (`vt100`/`avt`) para capturar a tela após sequências de render.
- Testar: resize, wide chars (CJK/emoji), tema, foco, main vs alt, bracketed paste, mouse.
- Golden files por largura; atualizar via `cargo insta review`.
- Captura de ANSI bruto para diagnóstico (equivalente a `PI_TUI_WRITE_LOG`).

### 2.6 Testes de compatibilidade
- **Sessão**: gerar sessão no Pi (Node) → abrir no `pi-rs` → comparar contexto; e vice-versa.
- **RPC**: cliente do Pi original (ou um cliente de referência em Python do docs) contra `pi-rs`; ClientMessageDecoder/ServerMessageDecoder contra frames do `pi-protocol` original.
- **Model catalog**: comparar catálogo gerado com o do original (diff estrutural).

## 3. CI

```yaml
# matriz
- cargo fmt --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --workspace
- cargo llvm-cov (coverage)
- cargo audit / cargo deny check
- testes de compatibilidade (rodam Node para gerar oráculo)
```

- Testes que exigem rede/LLM: opt-in por env (mesma convenção `PI_*`), pulados por padrão.
- Isolamento de home/tmp como no `test.sh` (usar `tempfile::TempDir` + env restrito).

## 4. Evals

- Manter `packages/evals` em Node; apontar para o binário `pi-rs` via CLI/RPC.
- Comparar lift docs vs host entre Pi original e `pi-rs`.
- Não portar a infra de evals no MVP.

## 5. Benchmarks

O original mede storage (timing/memória) e profiling de TUI/RPC. Em Rust:
- `criterion` para storage e parsing.
- `dhat` ou `heaptrack` para memória.
- Alvos: latência de append de sessão, throughput de eventos, tempo de frame TUI.

## 6. Regras de qualidade

- Erros: `cargo clippy` sem warnings.
- Cobertura do core (`ai`/`agent`) alta; TUI por golden.
- Nenhum teste depende de estado global do usuário (mesmo princípio do `test.sh`).
- Testes de regressão referenciam a issue/commit de origem.
