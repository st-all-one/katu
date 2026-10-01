# Q-15 · Estado persistente e retomada com cauda limitada (F5) — protocolo e resultado

## Pergunta

A retomada de uma sessão continua a **crescer com a história**? (Brecha (e) do
`OPTIMIZATION_PLAN.md` §1.4: `session.open` ≈ 8,5 ms/turno sem atribuição, F5.)

## Fórmula

O snapshot guarda `(seq, offset, estado, uso)`; a retomada lê o snapshot e **reaplica só a cauda**:

```
retomada = ler(snapshot) + replay(eventos com seq > snapshot.seq)
cauda    = log.offset − snapshot.offset        (bytes que o replay teria de ler)
```

O custo do replay é proporcional à **cauda**, não à história. Duas fronteiras escrevem snapshot:

1. `PhaseTransition` — o contrato do ADR 0008 (fronteira de fase);
2. `TurnEnd` **e** `cauda ≥ MAX_TAIL_BYTES` (128 KiB) — o teto que faltava (Q-15).

Sem (2), uma fase longa deixa uma cauda que cresce com a fase. Com (2), `cauda ≤ 128 KiB` por
construção, e o número de snapshots é `log_bytes / 128 KiB`.

O snapshot leva também o **hash canónico do estado** (FNV-1a da serialização determinística): um
snapshot que não casa com o próprio estado é descartado e faz-se *replay* total (fail-closed), em vez
de aceitar um estado plausível e errado.

## Cenário

Sessões sintéticas em `MemFs` com 2 000 e 20 000 turnos (3 eventos cada), mais um cenário com **5 tool
calls por turno** (10 000 chamadas acumuladas) para pesar o mapa de chamadas do estado. Três
políticas no **mesmo log**:

- `replay` — sem snapshot nenhum (replay total);
- `phase` — um só snapshot, na fronteira de fase a 90% (comportamento até Q-15);
- `bounded` — a produção (Q-15).

## Como correr

```sh
cargo run -q --release -p xtask -- bench-resume    # escreve bench/e18/resume/raw.json
cargo test -q -p katu-core --lib kernel::session   # invariantes (teto, hash, replay == estado)
```

## Resultado (artefacto `raw.json`, mediana de 7)

| Cenário | log | `replay` | `phase` | **`bounded`** | cauda |
|---|---|---|---|---|---|
| 2 000 turnos | 0,39 MB | 2 138 µs | 231 µs | 663 µs | 125 244 B |
| 20 000 turnos | 4,0 MB | 23 839 µs | 2 165 µs | **321 µs** | 61 812 B |
| 2 000 turnos × 5 calls | 3,2 MB | 26 430 µs | 2 668 µs | **575 µs** | 69 617 B |

- **20 000 turnos:** `bounded` **−98,7 %** contra o replay total e **−85,2 %** contra a política
  anterior (`phase`). A cauda ficou em 61 812 B com 30 snapshots.
- **2 000 turnos × 5 calls:** −97,8 % / −78,4 %.
- A retomada de 20 000 turnos (321 µs) é **mais rápida** que a de 2 000 turnos (663 µs): o custo
  deixou de depender do comprimento e passou a depender só de **onde caiu a última fronteira**.
- A cauda nunca passou de 125 244 B (< 128 KiB) — o teto é observável no artefacto.

## Custos

- **Escritas:** um snapshot a cada 128 KiB de log (30 numa sessão de 4 MB). No `MemFs` a escrita é
  barata; em disco real o E18 mediu ~3,7 ms por escrita atómica (`tmp → sync_all → rename`), ou seja
  **≈ 0,006 ms por turno** a 20 000 turnos. É o preço da retomada O(1); P-01 (`group-commit`)
  baixa-o ainda mais.
- **Uma serialização extra do estado** por snapshot (para o hash canónico). O snapshot é escrito
  ~30 vezes numa sessão de 4 MB: irrelevante face às 30 escritas atómicas.
- **Nenhum token de prompt**, nenhuma mudança *model-visible*: o snapshot não entra no log nem no
  pedido.

## Efeito colateral corrigido (medido)

O `State` guardava **todas** as chamadas (`CallStatus::{Pending,Done}`) e cada `step` clona o estado.
O mapa crescia com a sessão, pelo que o replay era **quadrático** — visível no cenário com chamadas:
26 430 µs só para 3,2 MB de log, contra 2 138 µs para 0,39 MB sem chamadas. Pior: o id de uma chamada
que o provider não identifica é `call_{index}` (por resposta), pelo que um id repetido num turno
seguinte era recusado como `DuplicateCall` — um bug latente (o modelo local não emite tool calls, por
isso nunca apareceu).

A correção é a mesma que resolve o custo: o estado guarda apenas as chamadas **pendentes**
(`State.pending`), o efeito vive no log e o nome concluído em `completed_tools`. A recusa de id
repetido passa a valer só para **dois pedidos vivos** com o mesmo id — que é a ambiguidade real.
Teste: `a_call_id_may_repeat_after_it_settles` (dois turnos com `call_0`).

## Limites

- `MemFs` não tem `fsync`: o custo das escritas de snapshot **não** está no artefacto (usa-se o número
  de escritas e o custo por escrita medido no E18).
- As sessões são sintéticas (turnos + tool calls `Ok`), sem conteúdo de tools: mede-se a **retomada**,
  não o trabalho do modelo.
- O teto é absoluto (128 KiB), não relativo: numa sessão curta (< 128 KiB) a política nova pode deixar
  uma cauda **maior** que a fronteira de fase (663 µs contra 231 µs a 2 000 turnos). O pior caso é
  ~1 snapshot de cauda (≤ 128 KiB) e está abaixo do ruído do baseline (`session.open` 8,5 ms).
- Ainda se serializa o estado inteiro por snapshot: uma estrutura persistente (partilha estrutural
  *path copying*) reduziria o custo do snapshot, mas não o do replay, que é o que se mede aqui —
  fica registado como próximo passo, não como promessa.
- `Session::verify` continua a ser o teste de consistência: `state_of(replay) == state` é afirmado no
  teste do teto (`the_tail_stays_under_the_cap_and_the_state_survives`).
