# 15 — Referência rápida

## 1. Comandos essenciais

| Objetivo | Comando |
|---|---|
| Fundar | `kd init [--force] [--no-prompt]` |
| Protocolo | `kd prime [--long]` |
| Buscar | `kd ask "<q>" [--brief] [--limit N] [--full-content] [--with-task]` |
| Corpos | `kd ask --id <ID>...` |
| Expandir | `kd ask --around <ID> [--via E] [--depth N]` |
| Confiáveis | `kd ask --rank --universe` |
| Tags | `kd ask --tags` |
| Sugestões | `kd ask --suggest [--relation duplicate\|contradiction\|link]` |
| Gravar | `kd write --summary "<s>" [body] --type T --tag T --anchor P` |
| Versionar nota | `kd write --update <ID> --summary "<s>"` |
| Aresta | `kd write --link "<FROM:EDGE:TO>"` / `--edge EDGE:ID` |
| Evidência | `kd write --outcome <success\|partial\|failure\|abandoned> --id <ID>` |
| Lote | `kd write --batch <file\|-> [--dry-run]` |
| Tarefa | `kd task new --summary "<s>" --scope <epic\|issue\|task>` |
| Listar tarefas | `kd task list [--ready\|--blocked [--explain]] [--sort impact]` |
| Fechar tarefa | `kd task close --id <ID> --outcome success --note "..."` |
| Árvore/fluxo | `kd task graph` · `kd task flow` |
| Handoff | `kd rewind [--budget N] [--files P] [--resume ctx]` |
| Mapa | `kd map [--axis ...] [--semantic] [--communities] [--write]` |
| Saúde | `kd doctor [--fix] [--explain]` |
| Manutenção | `kd maintenance <learn\|compact\|prune> [--universe]` |
| Fila | `kd drain [--status\|--digest]` · `kd drain service ...` |
| Config | `kd config <get\|set\|unset\|list\|promote>` |
| Esquecer | `kd forget --id <ID> [--restore\|--purge]` |
| Commit | `kd sync [--message M]` |
| Binário | `kd self <version\|setup\|completions\|upgrade>` |

## 2. Enums fechados (D212)

| Flag | Valores |
|---|---|
| `--type` | `fact decision question task def error snippet link meta risk` |
| `--class` | `foundational tactical observational` |
| `--status` | `active in_progress blocked closed superseded forgotten` |
| `--scope` | `epic issue task` |
| `--kind` | `task error question risk decision` |
| `--outcome` | `success partial failure abandoned` |
| arestas | `references depends_on contradicts supports extends replaces rejects results_in same_as broader narrower related` |
| `--relation` | `duplicate contradiction link` |
| `--axis` | `anchor type classification scope` |
| `--sort` | `impact` |
| `--template` | `feature bug refactor` |
| `self setup` | `claude cursor codex pi` |
| `self completions` | `bash zsh fish` |
| `--log-level` | `error warn info debug trace off` |

## 3. Exit codes

`0` ok · `2` invalid input · `3` not found · `4` conflict · `5` io · `6` timeout
· `7` config · `8` schema · `9` unsafe · `70` internal · `101` panic.
EPIPE → `0`.

## 4. Dedup e limiares

| Score | Ação |
|---|---|
| `< dedup.create_below` (0,75) | cria |
| `[0,75, dedup.merge_below)` (0,92) | merge |
| `≥ 0,92` | rejeita |

## 5. Fórmulas e constantes

| Modelo | Fórmula/constantes |
|---|---|
| **BM25** | `k1=1.5`, `b=0.75`; pesos `3.0/2.0/1.0`; `IDF` por campo; corte `df/N≥0.9` só se `N≥64` |
| **RRF** | `Σ peso/(k+rank+1)`, `k=60`; pesos `lexical 1.0`, `anchor 2.0`, `semantic 30.0`, `ppr 0.0`; tie `(score desc, id asc)` |
| **Confiança** | Beta(1+s,1+f); Wilson `Z=1.96`; `age=1/(1+a/90)`; `AGE_WEIGHT=0.05`; `feedback=0.2`; `contradiction=0.1`; `drift_factor=1−0.5·drift` |
| **Retenção** | FSRS-like; `limiar=0.5`; `growth=50%`; `R(t)=limiar^(t/ttl)`; `S=ttl/ln2` |
| **Drift termos** | Jensen-Shannon base 2, `smoothing=1e-9`; `threshold=0.5`, `min=4` |
| **PageRank/PPR** | `damping=0.85`, `≤32` iter, tol L1 `1e-8` |
| **Louvain** | `≤8` níveis, `≤64` passos, empate mantém comunidade |
| **MinHash/LSH** | 64 permutações, 16 bandas × 4 linhas, `MIN_LSH_CORPUS=256` |
| **PERT/CPM** | `L=lead+max L(deps)`; desempate total→maior caminho→menor lexicográfico |

## 6. TOON — regras de ouro

- Ordem canônica sempre; opcionais **omitidos**, nunca `null`.
- Inteiros nunca saem `.0`; raw UTF-8; sem BOM; `LF` final.
- Lista/mapa vazio omitido; documento vazio = 0 bytes; aninhamento ≤ 32.
- Escapes: `\"`, `\\`, `\n`, `\r`, `\t`, `\0`, `\u{XXXX}`.

## 7. Identidade

```
normalize(s) = NFC + trim + colapso de whitespace
body_hash    = hex8(SHA-256(normalize(statement) + LF + normalize(body)))
id           = <prefixo>_<base36(8)>(SHA-256(type + U+001F + normalize(statement)))
```

## 8. Layout em disco

```
.knudge/
  config.toml · notas/<tipo>/<id>.md · eventos/events*.jsonl
  templates.toml · validators.toml · emb_cache.jsonl · setup/
  .idx/ (derivado) · cache/ · .locks/
```

## 9. Ciclo de vida resumido

```
kd prime → kd ask → kd write → kd task → kd rewind → kd doctor → kd sync
(protocolo)(buscar) (gravar)   (executar) (retomar)   (saúde)     (commit)
```

## 10. Config — chaves de bolso

| Chave | Default |
|---|---|
| `recall.default_limit` | `5` |
| `recall.semantic` / `semantic_weight` | `true` / `30.0` |
| `recall.anchor_weight` / `rrf_k` | `2.0` / `60` |
| `dedup.create_below` / `merge_below` | `0.75` / `0.92` |
| `behavior.strict` | `false` |
| `embeddings.provider` / `mode` | `http` / `lazy` |
| `embeddings.model` / `dimensions` | granite / `384` |
| `embeddings.endpoint` | `http://127.0.0.1:8889/v1/embeddings` |
| `mcp.hints_cap` / `observation_mode` | `3` / `true` |
| `retention.growth_percent` | `50` |
| `programs.glob` | `plan/*.md` |

## 11. MCP

Tools: `knudge_pre_write`, `knudge_pre_edit`, `knudge_session_end`,
`knudge_status`. Protocolo `2025-06-18`. Configure com
`kd self setup <claude|cursor|codex|pi>`. Hints são **ponteiros**
(`id` + statement + score).

## 12. Documentos-fonte

- `wiki/usage/` — um guia por comando.
- `wiki/specs/` — técnica por subsistema (`TOON`, `ARCHITECTURE`,
  `matematica`, `DIVERGENCES`, …).
- `plan/implementation/16_cli_surface.md` — contrato da CLI.
- `plan/03_decisoes-fechadas.md` — decisões `D01–D214`.
- `SKILL.md` — guia do agente (copiado neste guia).
