# 06 — Grafo, ontologia, autoridade e TMS

O grafo é uma **projeção** das notas: a verdade continua em
`notas/<tipo>/<id>.md`. Aqui vivem arestas explícitas, integridade, ciclos,
autoridade (PageRank/PPR), comunidades, ontologia leve e TMS.

## 1. Arestas explícitas (D49/D51/D98)

As arestas são **chaves de frontmatter de primeiro nível**, nomeadas pelo
`EdgeKind`, cada uma uma lista de ids (omitida quando vazia):

```
references · depends_on · contradicts · supports · extends · replaces · rejects
· results_in · same_as · broader · narrower · related
```

- Criadas por **via única**: `kd write --link "<FROM:ARESTA:TO>"` (D126) ou
  `--edge ARESTA:ID` na criação.
- `superseded_by` é o **ponteiro reverso** (id único) de `replaces`; a
  bidirecionalidade é validada pela integridade.
- `link()` adiciona sem duplicar; rejeita id inválido e auto-aresta.
- A **extração textual** é **sugestão revisável** em `.idx/suggestions.jsonl` e
  **nunca** entra no grafo (D49/D50). `expand` percorre só o explícito.

Semântica das arestas de ontologia (D207): `broader`↔`narrower` são **inversos**;
`same_as`/`related` são **simétricos**.

## 2. Integridade e ciclos (D45/D46)

- `integrity.rs` reporta referências quebradas, órfãos e a bidirecionalidade
  `replaces ↔ superseded_by`.
- `cycles.rs` detecta ciclos por **SCC (Kosaraju iterativo)** sobre
  `replaces`/`depends_on`; membros de ciclo **não demovem** (proteção de
  supersessão, D45).

## 3. Autoridade: PageRank / PPR (D192)

`rank.rs` (puro, sem dependências) calcula **PageRank** global e **Personalized
PageRank** (semeado pelo working set) sobre as arestas de autoridade
(`references`/`supports`/`extends`/`replaces`).

```
r_{k+1}(v) = (1−d)·p(v) + d·Σ_{u→v} r_k(u)/outdeg(u) + d·D·p(v)
```

| Parâmetro | Valor |
|---|---|
| damping `d` | `0.85` |
| iterações máx. | `32` |
| tolerância L1 | `1e-8` |
| `p(v)` | uniforme (`1/n`) ou `1/|seeds|` (PPR) |
| `D` | massa **dangling** redistribuída por `p` |

Vira o canal `ppr` da fusão RRF (`recall.ppr_weight`, default `0.0` =
desligado). Só compensa em corpora com muitas arestas.

## 4. Comunidades (D193)

`communities.rs` implementa **Louvain determinístico** (*local moving* +
agregação) sobre grafo ponderado não-dirigido:

- ganho `ΔQ_{i→c} ∝ w_{i,c} − k_i·Σ_tot(c)/(2m)`;
- **empate mantém a comunidade corrente** (`EPSILON = 1e-9`);
- ≤ `MAX_LEVELS = 8` níveis, ≤ `MAX_PASSES = 64` passos por nível;
- agregação: arestas internas viram auto-laço com peso **dobrado** (`×2`);
- nós em ordem lexicográfica; comunidades ordenadas por tamanho desc e menor
  membro.

`lifecycle/communities.rs` monta o grafo a partir de arestas explícitas (peso 1)
+ **âncoras compartilhadas** (clique; estrela acima de 64 membros) e produz
`Community { members, terms }` com resumo local (`content_terms`, top 8).
Expõe em `kd map --communities` e materializa no `MAP.md`.

## 5. Ontologia leve (D207)

`ontology.rs` infere sobre as arestas de ontologia:

- `equivalence_classes` (clausura de `same_as`);
- `broader_ancestors` / `narrower_descendants` (clausura de hierarquia);
- `has_hierarchy_cycle` (ciclo em `broader`/`narrower`);
- `claim_conflicts` — contradição precisa por claims: mesma `(subject, relation)`
  com objetos divergentes; reportada pelo check `integrity` do `doctor`.

Tudo **derivado** — sem gravar nada, sem bump além do aditivo 1→2.

## 6. TMS / defeasible (D208)

`tms.rs` deriva do índice reverso de `depends_on`:

- `retracted(graph)` — premissas retratadas (`forgotten`/`superseded`);
- `defeated_dependents(graph, retracted)` — dependentes **transitivos** de
  premissas retratadas;
- `defeated_by_replacement(graph)` — alvos de `replaces`.

Sem apagar nada (D14): o lado substituído/contradito é **derrotado, não
removido**. Entra como motivo de demolição (`DemotionReason::Defeated`) no
`maintenance prune` — off-path, só propõe.

## 7. Estrutura do `Graph`

`Graph` mantém nós (por id) e arestas; expõe `parents` (índice reverso O(1) para
`parent`/`has_parent`), `is_work_item` (D120), `has_contradictions`,
`belongs_to`. `ExpandHit` representa a expansão por vizinhança.

## 8. Uso pela CLI

```bash
kd ask --around fact_01m81b6h                  # vizinhos diretos
kd ask --around fact_01m81b6h --via extends --depth 2
kd map --universe --communities --members
kd map --universe --semantic
kd task graph --root epic_01abc
```

`expand` é BFS determinística só no **explícito**, com corte por `depth` e filtro
por tipo.

## 9. Matriz de relações

| Aresta | Direção | Uso principal |
|---|---|---|
| `references` | → | autoridade/PPR |
| `depends_on` | → | `ready`/`blocked`, impacto, TMS, flow |
| `contradicts` | → | penalidade de confiança, prune |
| `supports` / `extends` | → | autoridade |
| `replaces` / `superseded_by` | → / reverso | supersessão, ciclos |
| `rejects` | → | curadoria |
| `results_in` | → | hierarquia derivada (pai→filho) |
| `same_as` / `related` | simétrica | equivalência / associação |
| `broader` / `narrower` | inversos | hierarquia de conceitos |

## 10. Invariantes

- A nota é a verdade; o grafo é derivado e reconstruível.
- Arestas só explícitas — nunca inferidas para dentro do grafo.
- Sugestões são derivadas, purgáveis e **advisory**.
- Membros de ciclo são protegidos de demolição.
