# 05 — Busca, ranking e recuperação (`kd ask`)

`kd ask` cobre seis modos num só comando: **recall**, **get**, **expand**,
**rank**, **tags** e **suggest**. Por padrão devolve **só conhecimento** (notas
sem `scope`); itens de trabalho entram com `--with-task` (para listar trabalho,
prefira `kd task list`).

A busca é **insensível a acentos** (`configuracao` acha `configuração`) e
reconhece variações (singular/plural, conjugações, stemming PT).

## 1. Modos e sintaxe

```
kd ask <QUERY> [filtros] [--limit N] [--brief] [--full-content] [--with-task]
kd ask -                                # lê a QUERY do stdin
kd ask --params '<json>'                # consulta + filtros de uma vez
kd ask --id <ID>...                     # get: corpos por id
kd ask --around <ID> [--via ARESTA] [--depth N]   # expand
kd ask --rank   [filtros] [--universe]
kd ask --tags   [--limit N]
kd ask --suggest [--relation R] [--top-k N]
```

`ask` sem nenhum modo (sem query/âncora/id/around) → uso + exit 2. `--id` de
nota ausente **degrada com aviso** (não é erro); `--around` de nota ausente é
`not_found` (exit 3).

## 2. Cascata de retrieval

```
consulta
  → filtros determinísticos      [corta o universo]
  → BM25 no resíduo              [lexical]
  → canal de âncoras             [match exato path/glob]
  → canal vetorial               [opcional, off-path]
  → canal PPR                    [opcional, default desligado]
  → fusão RRF                    [soma canais presentes]
  → penalidade de contradição
  → confiança derivada / idade / drift
  → snippet / why / formatação
```

Canal ausente/falho **degrada para o lexical com `warnings`**, nunca aborta —
salvo com `strict`.

## 3. BM25 (D35–D38/D173)

```
score(d,q) = type_weight(type_d) · (1 + 0,1·confirmação_d)
           · Σ_f w_f · Σ_{t∈q} IDF_f(t) · tf_f(t,d)(k1+1) / (tf_f(t,d) + k1·norm_f(d))
```

| Parâmetro | Valor |
|---|---|
| `k1` / `b` | `1.5` / `0.75` |
| `norm_f(d)` | `1 − b + b·len_f(d)/avg_len_f` |
| pesos de campo `w_f` | `statement 3.0`, `tags 2.0`, `body 1.0` |
| `IDF_f(t)` | `ln(1 + (N − df_f(t) + 0.5)/(df_f(t) + 0.5))` (por campo) |
| `type_weight` | `decision 1.20`, `error 1.15`, `fact 1.10`, `def/risk 1.05`, `task/question 1.00`, `snippet 0.95`, `link/meta 0.90`, `epic 0.70` |
| boost confirmação | `×(1 + 0.1·conf)`, `conf = success + 0.5·partial` (+ tarefas, D108) |
| corte de alta frequência | `df/N ≥ 0.9` **só se** `N ≥ 64` (D173) |

`df(t)` usa o **maior** `df` entre os campos (conservador). O índice invertido
(`postings::Postings`) vive **em memória**, nunca é persistido.

### Tokenização (D36/D122/D172/D206)

1. **ASCII explícita** (replica `\w`).
2. **Fold de diacríticos**: NFD + descarte de marcas → `café ≡ cafe`; casamento
   por **termo inteiro** (não prefixo). O `normalize` do schema **não muda**.
3. **Stopwords PT+EN** e fragmentos de 1 caractere.
4. **Stemming PT conservador**: corta sufixos flexionais/derivacionais, radical
   mínimo **4 bytes**, plural antes do derivacional.

O índice persiste o **radical**; o snippet usa **termos crus**.
`INDEX_FORMAT = "retrieval-v4"` invalida índices antigos (rebuild por `mtime`).

## 4. Âncoras (D81/D86/D135)

`anchors` alimenta um canal próprio: match exato de path/glob
(`GlobPattern`, DP de uma linha). O `content_hash` é derivado
(`.idx/anchors.jsonl`) e `verify-on-hit` invalida âncora citada. O canal só
entra na fusão quando **intersecta o working set**.

```bash
kd ask --anchor src/gateway.rs
kd ask --anchor src/gateway.rs,src/queue.rs
```

## 5. Fusão RRF (D81/D123/D179)

```
RRF(x) = Σ_c  peso_c / (k + rank_c(x) + 1),   k = 60
```

| Canal | Peso default | Decisão |
|---|---|---|
| lexical | `1.0` | D123 |
| âncoras | `2.0` | D179 (match exato > rank-1 lexical ruidoso) |
| vetorial | `30.0` | D123 (comprime ranks em corpus pequeno) |
| PPR | `0.0` (desligado) | D192 |

Desempate determinístico `(score desc, id asc)`. `rrf_k` medido **inerte** no
corpus rotulado; o lever real é `anchor_weight`.

## 6. Ranking sem query — `--rank` (D107/D175)

Ranqueia por **confiança derivada** (evidência Beta + idade + drift), ordem
`(confidence desc, id asc)`. A idade entra **aditiva** (`AGE_WEIGHT = 0.05`)
quando `similarity = 0`; o drift desconta o score. Exige escopo ou `--universe`.

```bash
kd ask --rank --universe --limit 10
kd ask --rank --tag retry --limit 5
kd ask --rank --anchor src/gateway.rs
```

## 7. Contradição (D177)

O **perdedor** de uma aresta `contradicts` declarada (menor confiança derivada)
é rebaixado por `CONTRADICTION_PENALTY = 0.1`. Empate **não** elege perdedor
(determinístico).

## 8. Filtros e views

Filtros determinísticos aplicados **antes** da estatística (D41/D53):
`--type`, `--class`, `--tag`, `--status`, `--scope`, `--anchor`, `--since`,
`--until`, `--as-of`. Eles se **somam** (interseção).

- **Views** `ready`/`blocked` derivadas do `depends_on` transitivo; ciclo =
  `blocked` (menor id pendente).
- `Status::VISIBLE` (`active`/`in_progress`/`blocked`/`closed`) é o default;
  `forgotten`/`superseded` ficam de fora — inclua com `--status`.
- `--scope <ID>` traz os membros do épico.

## 9. Consulta temporal — `--as-of` (D155)

Reconstrói o conjunto **ativo em `T`** a partir do log de eventos
(`forget`/`restore` + `link replaces`).

```bash
kd ask "postgres" --as-of 2026-07-01T00:00:00.000Z
kd --json ask "postgres" --as-of 2026-07-01 | jq '.data.historical'
kd ask "postgres" --as-of 2099-01-01   # futuro → exit 2
```

`T` sem eventos → `[no_results]`; `--json` traz `as_of`/`historical`.

## 10. Saída, snippet e `why`

- **Pipe:** `id|statement|score|why`; no `--rank`:
  `id|statement|confidence|why`.
- **Corpo progressivo** (D161): 1º hit com corpo completo; 2º–5º truncado a
  `recall.preview_chars` (default 280); 6º+ no padrão. `--brief` desliga;
  `--full-content` mostra tudo.
- **`why`** é fechado (D39):
  `file_match > anchor_match > tracker_match > stars > semantic > recent >
  universal`.
- **`channels`** (`--json`, D151): parcelas RRF (`lexical`/`anchor`/`semantic`)
  + boosts (`recent`/`stars`); `body_match`/`body_snippet`.
- Busca vazia → stdout `[no_results]` literal (exit 0); `--json` com `hits: []`.
- `recall.default_limit = 5`.

## 11. Modo `--tags`

```bash
kd ask --tags            # tag|count, da mais usada à menos
kd ask --tags --limit 20
```

## 12. Modo `--suggest` (D158) — advisory

Sugere pares semelhantes, classificados em três relações **fechadas**:

| `--relation` | Quando aparece | Significado | Ação típica |
|---|---|---|---|
| `duplicate` | similaridade ≥ `dedup.merge_below` (0,92) | quase-duplicata | merge/`--update` |
| `link` | ≥ `suggestions.contradiction_high` (0,75) **ou** banda `0,40–0,75` **com** âncora comum | relacionadas sem aresta | `write --link` |
| `contradiction` | banda `0,40–0,75` **sem** âncora comum | mesmo tópico, possível contradição | revisar; talvez `contradicts` |

Saída: `relação|from|to|score`. Pares que **já têm** aresta nunca aparecem.
Nada vira aresta sozinho; sugestões vivem em `.idx/suggestions.jsonl` (derivado,
purgável, D50).

## 13. Consulta em objeto (automação)

```bash
kd ask --params '{"query":"cache","limit":3,"brief":true}'
echo '{"query":"gateway","type":["decision"]}' | kd ask --params -
kd --json ask "cache" --limit 5 | jq '.data.hits[].id'
```

## 14. Quando não usar

- Criar/editar → `kd write` / `kd task update`.
- Histórico de sessão → `kd rewind`.
- Listar trabalho → `kd task list`.

## 15. Referência de flags

| Flag | Efeito |
|---|---|
| `[QUERY]...` | consulta textual (`-`/pipe lê stdin) |
| `--params <JSON>` | consulta + filtros (`-` lê stdin) |
| `--id <ID>...` | modo **get** (repetível; aceita vírgula) |
| `--around <ID>` / `--via <ARESTA>` / `--depth <N>` | modo **expand** |
| `--brief` | saída mínima `id\|afirmação` |
| `--full-content` | corpo completo dos hits |
| `--with-task` | inclui itens de trabalho |
| `--type/--class/--tag/--status/--scope/--anchor` | filtros (repetíveis/vírgula) |
| `--since <TS>` / `--until <TS>` | janela de criação |
| `--as-of <TS>` | corpus ativo naquele instante |
| `--limit <N>` | limite de resultados |
| `--rank` / `--tags` / `--suggest` | modos |
| `--universe` | varredura explícita do projeto |
| `--top-k <N>` / `--relation <R>` | no modo `--suggest` |
