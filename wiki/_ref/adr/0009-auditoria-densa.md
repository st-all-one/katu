# ADR 0009 — Auditoria densa e pesquisável (`.katu/audit`, completa sob compactação)

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF1, DF5, DF9
- **Épicos:** E04 (log), E09-T07 (compactação), E10 (CLI), E18-F3/F9
- **Relaciona:** [ADR 0005](0005-formato-colunar-d39.md), [ADR 0006](0006-toon-colunar-v3.md),
  [ADR 0008](0008-sessoes-identidade-e-retomada.md)

## Contexto

Precisamos de uma **auditoria permanente** de tudo o que foi abordado, **completa mesmo após
compactação**, **extremamente densa** e **rápida/precisa de pesquisar** (consultas
pós-compactação). Vive em `<projeto>/.katu/audit/`, **só local** (nunca versionada).

A compactação (E09-T07) **não** deve apagar história: só substitui a *vista do modelo* por um
digest, mantendo o original endereçável (`recover`). A auditoria tem de sobreviver a isso.

## Justificação do desenho (a melhor escolha para consulta)

| Opção | Densidade | Latência de consulta | Precisão | Veredicto |
|---|---|---|---|---|
| (a) JSONL linha-a-linha | baixa (chaves repetidas) | O(N) varredura | filtros em pós-processo | rejeitada |
| (b) Colunar sem índice | alta | O(N) varredura | boa (filtros) | insuficiente sozinha |
| (c) **Colunar + índice invertido com posições** | **alta** | **O(candidatos)** (postings) | **frase + campos** | **adotada** |
| (d) SQLite/FTS embutido | alta | O(candidatos) | alta | rejeitada (dep nova, não determinística, firewall) |
| (e) Motor de retrieval (knudge/BM25) | — | — | — | rejeitada (o knudge é o motor de memória; não duplicar motores) |

**Decisão: (c).** O índice invertido converte a varredura `O(N)` em `O(candidatos)`, dá
**posições** (frase), **máscaras de campo** (path/tool/status/rule/msg/content) e permite
interseção/unção/ordenação determinísticas. Os **segmentos colunares** mantêm o armazenamento
denso e tornam barato materializar **uma** linha. O índice é **derivado e reconstruível** — uma
corrupção é um `rebuild`, nunca perda de dados. Bloom por segmento evita abrir segmentos sem o
termo. Nada disto cria um segundo motor de retrieval: é o **índice lexical do próprio histórico**.

## Decisão (formato e algoritmo)

1. **Registos — segmentos imutáveis, colunares** (`seg-<NNNNNN>.rec`):
   reutiliza o registo [`toon::schema`](../../../crates/katu-core/src/toon/schema.rs). Tabela `a`
   (uma linha por evento): `seq,kind,tool,path,status,rule,text`; o `text` é um excerto limitado
   (`MAX_TEXT_BYTES`) e o corpo integral fica no log. Segmento selado a cada `SEGMENT_EVENTS`
   eventos (256) e **nunca reescrito**.
2. **Índice invertido — binário `KAI1` (delta+varint) + Bloom por segmento** (adotado por A/B):
   - `seg-<NNNNNN>.idx`: magic `KAI1`, `bloom_len` varint, bits do Bloom, `term_count` varint e,
     por termo (ordem canónica), `term_len`+bytes, `count` e postings com `field` (u8), `ln` em
     **delta** e `pos` absoluto, ambos varint LEB128;
   - **Bloom** (4 hashes FNV-1a, ≈10 bits/termo) por segmento: a consulta **não abre** um segmento
     quando um termo obrigatório está garantidamente ausente (sem falsos negativos);
   - **A/B (DF5, dev-only)**: índice binário **76–79% menor** que a tabela `t` colunar
     (100k eventos: 4,85 MB vs 23,9 MB) e `decode` ~8× mais rápido que `Index::build`
     (0,29 s vs 2,4 s a 100k) — `cargo run -p xtask -- bench-audit`;
   - determinístico (`BTreeMap`), sem `HashMap` iterado e sem RNG.
3. **Manifesto** (`manifest.json`): `{schema, segments:[{id,from,to,events,rec_hash,idx_hash}],
   terms}`. É a raiz para `is_fresh`/`rebuild`.
4. **Consulta**: termos e frases; `AND`/`OR`; filtros `kind:`/`tool:`/`phase:`/`path:<glob>`;
   ordenação por **recência** e `seq` (determinística). Devolve a tabela `audit.hits`
   (`seg,ln,ts,kind,tool,path,status,preview`) — **ponteiros**, não corpos.
5. **Completude sob compactação**: a auditoria é append-only a partir do **mesmo** stream de
   eventos e **não** é tocada pela compactação; as linhas de `replacements` entram como eventos
   `compaction`, pelo que até o que o digest resumiu fica pesquisável e endereçável (`recover`).
6. **Local, não versionado**: `.katu/audit/` é adicionado a `<projeto>/.git/info/exclude`
   (idempotente). Retenção cresce para sempre; poda manual/gated.
7. **Segurança/determinismo**: redação (`diag::redact`) antes de persistir; sem RNG; ordem
   canónica; `Metric` (DF5) para bytes/evento e latência de consulta.

## Alternatives considered

1. **Só colunar, sem índice.** Rejeitada: a consulta pós-compactação varreria a história toda.
2. **Só JSONL.** Rejeitada: densidade e latência piores.
3. **SQLite/FTS.** Rejeitada: dependência pesada, não determinística, e a fronteira de
   `katu-core` é LLM/dep-light (firewall).
4. **Reusar BM25/knudge.** Rejeitada: o knudge é o motor de memória das **notas**; duplicar
   motores é cicatriz (§20).
5. **Índice como fonte de verdade.** Rejeitada: o índice é derivado; a fonte é o log/segmentos.
6. **Sem posições (só `tf`).** Rejeitada: perdia consultas de frase (escopo completo pedido).

## Consequências

- **Positivas:** consulta em `O(candidatos)`; frase/campos/filtros; densidade colunar; **índice
  binário 76–79% menor** e leitura offline do Bloom; reconstruível; completude estrutural sob
  compactação; nada versionado.
- **Negativas/dívida:** o índice tem de ser validado (`is_fresh`) e reconstruído quando o formato
  muda; o `manifest.json` ainda não guarda `rec_hash`/`idx_hash` (selagem é a ordem do manifesto).
- **Medição (DF5):** `bench-audit` mede bytes (colunar `t` vs binário) e o custo de
  `build`/`encode`/`decode` a 1k/10k/100k eventos (números acima); a latência p50/p95 de consulta
  com/sem Bloom fica para o próximo A/B.
- **Travas:** testes de segmento/índice/consulta (determinismo, frase, filtros, rebuild),
  `check-layers` (sem dependência de provider) e `check-diag`.
