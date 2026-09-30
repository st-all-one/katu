# ADR 0008 — Sessões: identidade, vinculação ao projeto, snapshot e retomada

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF1 (caminho único), DF5 (evidência), DF9 (diagnóstico)
- **Épicos:** E04 (kernel/sessão), E09-T02 (checkpoint), E10 (CLI), E18-F5 (estado/replay)

## Contexto

O kernel já tem um log append-only por **diretório** (`session.v1.jsonl`, `seq` contíguo,
fail-closed) e o estado é a sua projeção ([`state_of`]). O `checkpoint.json` é um artefacto de
fase. Faltava: **identidade** de sessão, **vinculação** ao projeto (para reabrir no path exato),
**ordenação temporal** e **retomada** sem reprocessar toda a história.

## Decisão

1. **Layout `.katu/` na raiz do projeto** (descoberta subindo até `.katu` ou `.git`):
   ```
   <projeto>/.katu/sessions/<id>/session.v1.jsonl   # log append-only (fonte da verdade)
   <projeto>/.katu/sessions/<id>/snapshot.v1.json   # State no último limite de fase
   <projeto>/.katu/sessions/<id>/meta.json          # id, root, created_ms, updated_ms, phase, turn, goal
   <projeto>/.katu/sessions/index.jsonl             # id → {root, created_ms, updated_ms, phase, turn}
   <projeto>/.katu/audit/…                          # ADR 0009
   ```
2. **`SessionId`** = `s_<16hex>` derivado **uma vez, na borda**, de `(root canónico, created_ms)`
   (FNV-1a; sem RNG no kernel). Persiste em `meta.json` e no `index.jsonl`.
3. **Ordenação temporal**: `index.jsonl` é a lista canónica de sessões; a ordenação é
   `(created_ms, id)` (e `updated_ms` para "recentes"). `list()` devolve já ordenado.
4. **Snapshot do `State` agora** (F5): a cada **transição de fase** (e no fecho de turno), grava-se
   `snapshot.v1.json` atomicamente com `{seq, state}`. `resume` = carregar o snapshot + reaplicar
   só os eventos com `seq > snapshot.seq` (O(deltas)). Sem snapshot, replay total (comportamento
   anterior).
5. **Retomada exata**: `resume(id)` restaura `root` (path exato), `State`, orçamento e o contexto
   (via `assemble`/`compact`) — "último estado exato".
6. **Auditoria local, nunca versionada**: a criação de `.katu/` garante, de forma **idempotente**,
   a linha `.katu/audit/` em `<projeto>/.git/info/exclude` (append; nunca `.gitignore` — o audit
   vive só no ambiente local).
7. **Nada apagado automaticamente**: retenção cresce para sempre; poda é manual/gated.

## Alternatives considered

1. **ID aleatório (UUID).** Rejeitada: RNG no caminho de criação; o contrato de determinismo
   (E18 §0.2) prefere hash determinístico do par (root, instante da borda).
2. **Sessão por diretório arbitrário (como hoje).** Rejeitada: sem vinculação ao projeto nem
   reabertura estável por id.
3. **Retomada por replay total.** Adiada: aceitável no início, mas O(história); o snapshot no
   limite de fase mantém o custo `O(deltas)`.
4. **`.gitignore`.** Rejeitada: versionaria a decisão; o audit é local do dev →
   `.git/info/exclude`.
5. **Guardar tudo dentro do repositório e versionar os logs.** Rejeitada: viola "audit só local".

## Consequências

- **Positivas:** reabertura rápida e ordenável no tempo; `resume` exato; audit local sem poluir o
  git; snapshot reduz o custo de retomada.
- **Negativas/dívida:** o snapshot pode divergir do log — a invariante `state_of(replay)` é
  validada na retomada (`verify`) e o snapshot é reconstruível; a poda fica gated.
- **Travas:** `session::identity`/`snapshot` com testes determinísticos (`MemFs`); `index.jsonl`
  canónico; `check-diag`/firewall intactos.
