# 08 — Handoff e `kd rewind`

`prime` é o **protocolo estático** (byte-idêntico por versão, D57); `rewind` é o
**estado dinâmico**: situa o próximo agente/rodada com o mínimo de tokens e um
`context_id` retomável 1:1.

## 1. Modos (`RewindMode`)

| Modo | O que devolve |
|---|---|
| `Manifest` | manifest curto (~30 tokens) — estado do projeto |
| `Scope(String)` | escopo por container/domínio |
| `Files(Vec<String>)` | working set a partir de arquivos alterados (`--files`) |
| `Auto` | detecta o escopo automaticamente |

## 2. `kd rewind`

```
kd rewind [--scope C] [--files PATH]... [--budget N]
          [--since TS] [--until TS] [--resume CONTEXT_ID]
          [--tag T]... [--anchor P]... [--type T]... [--class C]...
          [--around ID] [--depth N]
```

Exemplo de manifest:

```
notes=526 ready=304 blocked=0 containers=59 clean
recent: fact_01ibc4s4 decision_0022xuwr task_01pog2v9
next: task_00aqet3d|B2.1: PHPStan nivel 0 task_005y362k|B2.2: ...
fresh: stale=0 expiring=0 pending=467
```

- `notes`/`ready`/`blocked` — panorama;
- `recent` — notas mais novas;
- `next:` — próximas tarefas `ready` abertas, por impacto;
- `fresh:` — `stale`/`expiring`/`pending` (indexação).

### Exemplos

```bash
kd rewind --budget 2000
kd rewind --files src/gateway.rs
kd rewind --files src/gateway.rs --files src/queue.rs
kd rewind --scope epic_01abc
kd rewind --tag retry --anchor src/gateway.rs
kd rewind --around fact_01abc --depth 2
kd rewind --since 2026-01-01 --until 2026-06-01
kd rewind --resume <context_id>
kd --json rewind --budget 2000 | jq -r '.data.context_id'
```

## 3. Manifest e trust-tier (D106)

- `manifest.rs` ranqueia itens por **trust-tier**
  (foundational → decision → …).
- `next.rs` adiciona as linhas **dinâmicas** `next:`/`fresh:`; o `prime`
  permanece estático.
- Anexa o **corpo** de notas `foundational`/`decision` — o "porquê" chega junto
  do resumo.

## 4. Orçamento sem tokenizer (D40/D82)

- Heurística `ceil(len/4)` tokens.
- `DEFAULT_BUDGET = 4000`; `MIN_TAIL = 100`.
- Aplicado **item a item**: trunca o último e ignora sobra < 100 tokens.
- O excedente aparece como `dropped`.

## 5. `context_id` (D88)

- `derive_id` gera `ctx_<base36>` (`CONTEXT_PREFIX = "ctx"`), persistido em
  `.idx/contexts/`.
- `kd rewind --resume <id>` devolve o **mesmo contexto 1:1**, sem re-busca —
  handoff reprodutível entre agentes/rodadas.

## 6. Escopo automático (D41/D143)

- `scope.rs`: auto-context-scope (`git status -uall` + active work) e auto-flip
  (`FLIP_NOTES=100`/`FLIP_CONTAINERS=5`).
- Filtros de corpus (`--tag`/`--anchor`/`--type`/`--class`/`--around`) escopam o
  handoff.
- `impact`/`compute_views` são computados **uma vez**.

## 7. `--json`

```json
{ "context_id": "ctx_...", "items": [ ... ], "dropped": [ ... ],
  "embeddings_pending": 0, "flow": { ... } }
```

## 8. `prime` vs `rewind`

| | `prime` | `rewind` |
|---|---|---|
| Natureza | protocolo **estático** | estado **dinâmico** |
| Conhece o corpus? | não | sim |
| Byte-idêntico? | sim, por versão | não |
| Serve para | colar no início da sessão | "onde eu estava?" |
| `--long` | gramática TOON + schema | — |
| `--resume` | — | handoff 1:1 |

## 9. Quando não usar

- Buscar um assunto → `kd ask`.
- Listar trabalho → `kd task list`.
- `prime` não é busca nem histórico: descreve **como usar**, não **o que** você
  já sabe.

## 10. Fluxo típico

1. Ao **abrir** a sessão: `kd prime` (uma vez) + `kd rewind --budget 2000`.
2. Ao **editar** um arquivo: `kd rewind --files <path>` ou
   `kd ask --anchor <path>`.
3. Ao **encerrar**: `kd rewind` para deixar `next:`/`fresh:` prontos e
   `kd sync`.
