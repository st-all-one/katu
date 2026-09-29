# 04 — Escrita, dedup, update, arestas e ciclo de vida soft

## 1. `kd write` — toda a escrita

Cria notas, versiona (`--update`), liga notas (`--link`), anexa evidência
(`--outcome`), registra claims/proveniência e aceita lotes. **Trabalho não entra
aqui**: `--type task` é rejeitado (exit 2) — use `kd task`.

O posicional é o **corpo** (Markdown); a afirmação vai em `--summary`.

```
kd write [BODY]... --summary <TXT> [--type T] [--tag T]... [--anchor P]...
         [--class C] [--status S]
kd write --update <ID> [--summary TXT] [...] [--clear-anchors] [--params JSON]
kd write --link <FROM:ARESTA:TO>
kd write --edge <ARESTA:ID>
kd write --outcome <OUTCOME> --id <ID> [--note TXT]
kd write --claim S:R:O [--agent N] [--activity N]
kd write --batch <FONTE|-> [--dry-run]
kd write --params '<json>' [--dry-run]
```

Exemplo de saída (texto): `created|fact_01qejflt|r1` (ação, id, revisão).

### Dedup automático

| Situação (score) | Ação |
|---|---|
| nada parecido (`<0,75`) | `created` |
| parecido (`0,75–0,92`) | `merged` |
| muito parecido (`≥0,92`) | `rejected` |

### Corpo esperado por tipo

O `write` confere os **slots mínimos** de corpo (D191). Faltar seção gera
**aviso**, não erro (com `behavior.strict=true`, o aviso vira erro).

```bash
kd write --summary "Cache usa LRU" --type decision <<'EOF'
Alternativas: FIFO e LFU.
Por quê: custo O(1) e melhor taxa de acerto no padrão de acesso.
EOF

kd write --summary "O gateway faz retry exponencial" --type fact \
  --tag gateway --anchor src/gateway.rs
```

## 2. Idempotência e duas fases (D01/D26/D80)

O `id` é endereçado por `type + U+001F + normalize(statement)`: reescrever a
mesma afirmação **não duplica**. Se a chave muda, cria novo `id` +
`superseded_by`.

```
Draft
  → validação de forma (chave/tipo desconhecidos rejeitados)
  → fase 1: recall lexical de candidatos (dedup on-write)
  → decisão calibrada: <0,75 cria · [0,75,0,92) merge · ≥0,92 rejeita
  → merge/update + evento
  → purge/flush derivados
```

- **Dedup on-write** para notas; **on-read** para `events.jsonl` (D26).
- **Lexical** no write; o **semântico é eventual** (reconciliação) e nunca
  bloqueia (D80).
- Limiares vêm de `dedup.create_below` / `dedup.merge_below`.

### Dedup escalável (D204)

`propose_merges` escolhe a estratégia:

| Condição | Estratégia |
|---|---|
| corpus pequeno/esparso | **peneira exata** de postings — byte-idêntica |
| > `MIN_LSH_CORPUS` (256) **e** vocabulário denso | **MinHash (64 perm.) + LSH (16 bandas × 4 linhas)** |

LSH é aproximado: pares com Jaccard ≥ 0,92 têm prob. ~1 de compartilhar banda;
os candidatos ainda passam pelo **Dice exato** e pelo limiar de merge. As
propostas são **idênticas ou superconjunto** (recall ≥) — nunca perdem um par
real. Ganho medido: `propose_merges` denso N=1000 ≈ **−97 %**; `compact`/`doctor`
≈ **−94 %**.

## 3. Merge de campos

`write/merge.rs` funde numa nota existente **sem sobrescrever o corpo**: união de
`tags`/`anchors`/`outcomes`, `revision++` e claims/proveniência sem duplicar. A
`confidence` nunca é armazenada (D142).

## 4. Update e supersede (D01/D21/D48)

- `write::update` aplica um `Patch` versionado; cada update incrementa
  `revision`.
- `--params '<json>'` aceita `type/statement/body/tags/anchors/classification/
  status/scope/claims/provenance` + `--clear-anchors`.
- **Mudar o `type` ou a afirmação cria uma nova nota** e marca a antiga como
  substituída (`replaces` + `superseded_by`, bidirecional).
- `history` lista as versões.

```bash
kd write --update fact_01m81b6h --summary "Retry exponencial com jitter"
kd write --update fact_01m81b6h --params '{"body":"novo corpo","tags":["cache"]}'
kd write --update fact_01m81b6h --clear-anchors
```

## 5. Âncora × aresta

| | `--anchor` | `--link`/`--edge` |
|---|---|---|
| Liga a | arquivo/glob | outra nota (id) |
| Vocabulário | livre | fechado (12 relações) |
| Direção | — | dirigida (`from → to`) |
| Serve para | recuperar por arquivo, medir drift | navegar, planejar, curar, inferir |

As **12 relações**: `references`, `depends_on`, `contradicts`, `supports`,
`extends`, `replaces`, `rejects`, `results_in`, `same_as`, `broader`,
`narrower`, `related`.

```bash
kd write --link "decision_01abc:extends:fact_01xyz"
kd write --link "task_01def:depends_on:task_01ghi"
kd write --link "fact_01aaa:contradicts:fact_01bbb"

# aresta a partir da nota recém-criada (sem saber o id depois)
kd write --summary "O retry tem jitter" --type fact --edge extends:fact_01xyz
```

Valor fora das 12 relações → exit 8, com sugestão da mais provável.

## 6. Evidência (`--outcome`)

```bash
kd write --outcome success --id task_01def --note "testes verdes em CI"
kd write --outcome failure --id fact_01xyz --note "medição refutou a hipótese"
kd write --outcome partial --id task_01def --note "2 de 3 casos cobertos"
```

`outcomes[]` vale para **qualquer** nota. Sucesso **fortalece** (aparece em
`ask --rank`); falhas enfraquecem. Os outcomes alimentam a confiança Beta
(§`09`) e a retenção FSRS-like.

## 7. Claims e proveniência (opcional, D207)

```bash
kd write --summary "embeddings escuta na 8889" --claim "embeddings:porta:8889"
kd write --summary "..." --agent claude --activity write
```

Claims habilitam detecção **precisa** de contradição: duas notas com o mesmo
`(subject, relation)` e objetos diferentes são apontadas pelo `doctor`.

## 8. Lote (D110/D141)

```bash
kd write --params '{"statement":"Cache expira em 30 dias","type":"fact","tags":["cache"]}'

kd write --batch - --dry-run <<'EOF'
{"statement":"Nota A","type":"fact"}
{"statement":"Nota B","type":"decision","tags":["x"]}
EOF

kd write --batch notas.jsonl
```

O lote usa as **chaves canônicas** (`statement`, `body`, `type`, `tags`,
`anchors`, `classification`, `status`, `claims`, `provenance`), não os nomes das
flags. É **best-effort** com `warnings[]`; teto `write.batch_max` (default 100);
`--dry-run` só avalia.

## 9. Ciclo de vida soft (`forget`/`restore`/`purge`)

`kd forget` **não apaga**: marca `status=forgotten` (soft-delete); a nota sai da
busca por padrão, mas continua no disco e no git.

```
kd forget --id <ID> [--restore | --purge [--force]]
```

- `--restore` desfaz (volta a `active`).
- `--purge` remove fisicamente **após a carência** (só para nota aposentada).
  `--force` exige tombstone (já esquecida/substituída) — **não** purga nota viva.
- `--purge` também remove arestas de entrada das outras notas (detach).

Saída: `forget|<id>|rN`, `restore|<id>|rN` ou `purge|<id>`. Nota ausente → exit
3; purga sem carência vencida → exit 2 (use `--force` para tombstones).

Transições de status são validadas (`status::validate_transition`); `superseded`
só via supersede, `forgotten` via `forget`.

## 10. Resumo de ações (`WriteAction`)

| Ação | Quando |
|---|---|
| `Created` | nada parecido |
| `Merged` | quase-duplicata (0,75–0,92) |
| `Rejected` | duplicata (≥0,92) |
| `Updated` | `--update` sobre id existente |

`--json` expõe `{action, id, revision}`; no lote, `items[]`.

## 11. Erros comuns

| Sintoma | Causa | Ação |
|---|---|---|
| `rejected` | duplicata | atualize a existente (`--update <ID>`) |
| `merged` | quase-duplicata | revise a nota resultante |
| `--type task` rejeitado | trabalho ≠ conhecimento | use `kd task new` |
| chave de rascunho desconhecida | `--params`/`--batch` usam canônicas | use `statement`, não `summary` |
