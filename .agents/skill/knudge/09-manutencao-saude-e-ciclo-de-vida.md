# 09 — Manutenção, saúde e ciclo de vida

Nada aqui escreve sem aceite (D33/D47): a manutenção **propõe**, o `doctor`
**repara o reversível**, e o ciclo de vida **sinaliza** o que aposentar.

## 1. `kd doctor` — saúde do corpus (D163)

```
kd doctor [--fix] [--explain]
```

Roda **13 checks + auditoria** num só relatório. Cada linha é
`ok|warn|fail <verificação> <mensagem>`.

- `healthy` só com **zero achados**; advisórios → `degraded` (nunca "saudável
  silencioso").
- `--explain` detalha cada achado (`esperado`/`encontrado`/`ação`).
- `--fix` corrige o **reversível e idempotente**: `body_hash`, âncoras
  quebradas, locks stale, duplicatas, `type: epic`, layout plano, índice
  divergente.
- O doctor **nunca apaga notas**.

Checks notáveis: `integrity` (referências/bidirecionalidade + **claims/
ontologia**), `body` (corpo + **slots mínimos**), `program-anchor` (warn),
`contradictions`, `duplicates`, `stale locks`.

```bash
kd doctor
kd doctor --fix
kd doctor --explain
kd --json doctor --fix | jq '.data.fixed'
kd --json doctor | jq '.data.duplicate_pairs'
```

`--json`: `{checks[], healthy, degraded, status, fixed[],
audit{duplicate_pairs, broken_anchor_details, missing_edge_details,
stale_lock_details, integrity_issues, ...}, suggestions[]}`.

## 2. Validators (D54/D99/D156)

Catálogo em `.knudge/validators.toml` (subset TOML próprio, D97). Cada validator:
`cmd` (obrigatório), `scope` (globs), `severity` (`error|warn|info`), `timeout`
(ms, default 120000) e `kind` (`check|gate`).

- Resolução: `checks(task) = explícitos ∪ globals ∪ por_âncora(anchors(task))`;
  validator explícito ausente vira `missing[]` (não é fatal).
- A **execução** fica na borda (`HookRunner`); o núcleo só resolve e descreve.
- **Gate** (D156): stdin `{op,before,after}` → stdout
  `{passed,score_before,score_after}`; decisão pura em `health/gate.rs`.
  `learn`/`compact --verify` anexam o veredito (`gate=passed|failed`).
  `proposals.enforce=true` faz o `pre-record` bloquear com `conflict` (4).
  Gate ausente/timeout/JSON inválido **degrada com aviso**.

## 3. Evidência e fechamento (D48/D55)

`close_task` roda os validators e **infere o `outcome`** pela severidade; sem
evidência, não fecha. A confirmação é **derivada** (nunca armazenada).

## 4. Leitura tolerante (D16–D18)

- Chave desconhecida no read: **tolera** com warning.
- Tipo desconhecido: **rejeita a nota** (por-nota).
- Nota/linha malformada: **skip com warning + orientação**.

`TolerantRead` reporta `SkippedNote[]` — nada é perdido em silêncio.

## 5. `kd maintenance` — propostas (D33/D47/D111/D112/D156)

Nenhum subcomando altera o corpus; todos exigem filtro ou `--universe`.

| Subcomando | Propõe |
|---|---|
| `compact` | fundir/marcar quase-duplicatas |
| `learn` | criar notas/links/merges a partir do que aconteceu |
| `prune` | aposentar notas obsoletas (shelf-life, âncora quebrada, contradição, drift) |

### `diff` (D33)

Lê a **auditoria de eventos** e devolve `DiffEntry[]` — o que mudou num
intervalo. Não escreve.

### `learn` (D33/D111)

Determinístico, a partir de eventos + âncoras + grafo + vetores. Propostas
`{kind, ids, why, score}` com `kind ∈ {create_note, merge, supersede, link}`:

| `LearnKind` | Sinal |
|---|---|
| `CreateNote` | atividade sem registro (write-gap) — inclui tarefa fechada sem nota |
| `Merge` | quase-duplicata |
| `Supersede` | substituição |
| `Link` | lacuna de grafo (aresta sugerida) |

### `compact` (D47)

Propõe fusões com estratégia:

| `CompactStrategy` | Efeito |
|---|---|
| `Concat` | concatena corpos |
| `KeepLatest` | mantém a mais recente |
| `MergeOutcomes` | funde outcomes |

`apply_compact` só materializa **após aceite** (escreve via `write`).

### `prune` (D112/D177/D208)

Propõe `forget|id|motivo` a partir de `demotion_candidates`, excluindo membros
de ciclo. A aplicação é sempre `kd forget`.

Saída: `estratégia|keep|ids|score` (compact) / `kind|ids|score` (learn) /
`forget|id|motivo` (prune). `--verify` anexa o portão.

## 6. Ciclo de vida e confiança

### Shelf-life e retenção (D44/D135/D190)

- `foundational` **nunca expira**; `tactical` (365 d) e `observational` (30 d).
- **FSRS-like:** cada outcome de **sucesso** estende o prazo em
  `retention.growth_percent = 50 %` e **reseta o relógio**
  (`origin = max(created, último ensaio, último uso se renew_on_use)`).
- `R(t) = limiar^(t/ttl)`; `limiar = 0.5`; o cruzamento `R = limiar` é o prazo.
  `stability_days = ttl / ln(1/limiar)`.

### Renovação por uso (D154)

Uso (citação) é derivado em `.idx/usage.jsonl`, nunca verdade. Com
`retention.renew_on_use=true`, a expiração é `max(created_at, last_seen) + prazo`
e **só estende**.

### Confiança Beta-Bernoulli + Wilson (D87/D189/D203)

- Outcomes viram ensaios: `success=1`, `partial=0.5`, `failure/abandoned=1`.
- Posterior `Beta(1+s, 1+f)`; a **média** alimenta `stars`; o **limite inferior
  de 95 %** (Wilson, `Z_95 = 1.96`) é a confiança conservadora.
- `confidence_score = clamp01((base + evidência + feedback 0.2 + tarefa +
  recência≤0.05) · drift_factor)`.
- `age_factor(a) = 1/(1 + a/90)` (`AGE_HALF_LIFE_DAYS = 90`);
  `AGE_WEIGHT = 0.05` só quando `similarity = 0`.
- `drift_factor = 1 − 0.5·drift ∈ [0.5, 1]`; desconta o score inteiro.

### Drift de âncoras (D43/D86/D203)

```
fração_válida = V/(V+B)
drift = 1 − fração_válida        ∈ [0,1]
drift_factor = 1 − 0.5·drift     ∈ [0.5,1]
```

Persistido em `.idx/drift.jsonl`; `prune` grava, `ask`/`rank` carregam. Arquivo
ausente ⇒ `drift = 0`.

### Drift de termos (D208)

Compara a distribuição de termos da metade antiga × nova (por `created_at`) de
cada tópico (âncora) por **Jensen-Shannon** (base 2, `SMOOTHING = 1e-9`).
Propõe revisão se `JS ≥ 0.5` **e** `≥ 4` notas.

### Demolição (`DemotionReason`, D45/D112)

| Motivo | Condição |
|---|---|
| `Expired` | shelf-life vencido |
| `AnchorDecay` | fração de âncoras válidas < threshold |
| `Contradicted` | lado perdedor de `contradicts` |
| `Defeated` | dependente de premissa retratada / alvo de `replaces` (TMS) |
| `Drifted` | vocabulário do tópico mudou |

Membros de ciclo são protegidos. A aplicação é sempre `kd forget`.

## 7. Clusters e comunidades (D47/D128/D129/D193)

- **Fase 1 — estrutural** (`clusters.rs`): agrupamento determinístico por eixo
  (`container`/`tag`/`anchor`/`type`/`classification`/`scope`); `scope_of` sobe
  pelos pais (`results_in`) com fallback `depends_on`.
- **Fase 2 — semântica** (`semantic.rs`): opcional, off-path, **complete-link** —
  um id só entra se for similar (≥ `clusters.similarity_threshold`) a **todos**
  os membros.
- **Comunidades** (D193): GraphRAG sobre arestas + âncoras compartilhadas.

## 8. `kd map` — panorama

```
kd map [--axis anchor|type|classification|scope] [--scope ESCOPO]
       [--semantic] [--communities] [--members] [--write]
       [--type T]... [--class C]... [--tag T]... [--anchor P]...
       [--around ID] [--depth N] [--universe]
```

Exige filtro ou `--universe`. Saída: `docs=<n> clusters=<n>` + linhas
`<eixo>|<chave>|<contagem>` (membros indentados com `--members`).

```bash
kd map --universe --axis type
kd map --tag retry --axis anchor
kd map --universe --axis anchor --members
kd map --universe --semantic
kd map --universe --communities --members
kd map --universe --write          # materializa notas/MAP.md + hubs
```

`--write` cria `notas/MAP.md` (árvore de grupos + clusters) e uma nota-hub por
cluster — versionadas e buscáveis. Sem embeddings, `--semantic` degrada com
aviso.

## 9. Fluxo de manutenção recomendado

```bash
kd doctor                              # diagnóstico
kd doctor --fix                        # reparar o reversível
kd maintenance learn --universe        # o que deveria virar nota?
kd maintenance compact --universe      # o que fundir?
kd maintenance prune --universe        # o que aposentar?
kd map --universe --axis type          # panorama
kd --json maintenance prune --universe | jq '.data.proposals[].reason'
```
