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

1. **Registos — segmentos imutáveis, colunares** (`seg-<NNNN>.rec`):
   reutiliza o registo [`toon::schema`](../../crates/katu-core/src/toon/schema.rs). Tabela `a`
   (uma linha por evento): `seq,ts,kind,tool,path,status,rule,bytes,ms,tok,msg`; conteúdo pesado
   (`stdout`/`diff`/`statement`/`preview`) em **blocos literais**. Segmento selado a cada `N`
   eventos **ou** limite de fase, com **hash de conteúdo** (nunca reescrito).
2. **Índice invertido — binário, delta+varint** (`seg-<NNNN>.idx`):
   - dicionário ordenado de termos: `term_len, term, df, offset_postings`;
   - postings: por termo, por documento `doc_delta varint`, `tf varint`, e por campo
     `field u8`, `pos_count varint`, posições em `delta varint` (posições → **frase**);
   - `magic KAIX` + versão; determinístico (`BTreeMap`), sem `HashMap` iterado.
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

- **Positivas:** consulta em `O(candidatos)`; frase/campos/filtros; densidade colunar; índice
  reconstruível; completude estrutural sob compactação; nada versionado.
- **Negativas/dívida:** implementar varint/Bloom e o merge de postings; o índice tem de ser
  validado (`is_fresh`) e reconstruído quando o formato muda.
- **Medição (DF5):** bytes/evento (colunar vs JSONL), latência p50/p95 de consulta a 1k/10k/100k
  eventos, custo de `rebuild`, e bytes do índice por evento — via `xtask` (dev-only).
- **Travas:** testes de segmento/índice/consulta (determinismo, frase, filtros, rebuild),
  `check-layers` (sem dependência de provider) e `check-diag`.
