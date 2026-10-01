# B-04 · Output ledger unificado (head/tail + spill) — protocolo e resultado

## Pergunta

O output de um comando (`bash`) que excede o orçamento de contexto é truncado de forma determinística
— sem que o modelo perca a capacidade de **recuperar** o output inteiro quando precisa?

## Fórmula

O [`Ledger`] (`feedback.rs`) é o mecanismo **unificado** de truncagem (B-04). Substitui as
truncagens ad-hoc por uma só regra:

```
model_visible = head(text, H) + marcador + tail(text, T)   se len > H + T
              = text                                        se len ≤ H + T
spill         = escrever text inteiro em <root>/.katu/spill/<id>.<stream>   se len > S
ponteiro      = " — ver <spill_path>"                        só quando spill
teto          = H + T + len(ponteiro)    (nunca excedido)
```

Parâmetros (`Ledger::DEFAULT`): `H = 2048`, `T = 2048`, `S = 8192` bytes. São **dados** (DF8),
não derivados da observação.

**Recuperação:** quando o output excede `S`, o texto inteiro é vertido para uma página de *spill*
sob a raiz do workspace e o ponteiro é incluído no texto model-visible. O modelo pode ler a página
com `read` — a informação não se perde, só sai do contexto.

## Cenário (canónico, determinístico)

Três cenários sintéticos, alimentados pelo `Ledger` de produção:

| cenário | input (bytes) | model-visible (bytes) | ponteiro |
|---|---|---|---|
| `small` | 8 | 8 | não |
| `head_tail` | 6 000 | 4 123 | não |
| `spill` | 20 000 | 4 153 | sim |

**Total:** 26 008 bytes de input → **8 284** bytes model-visible (31,8 %). O teto model-visible é
~4 153 bytes (head + tail + ponteiro), **independentemente** do tamanho do input.

## Como correr

```sh
KATU_LEDGER_OUT=$PWD/bench/e18/ledger/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_ledger_by_artifact
```

O invariante (teto de bytes + ponteiro) é asserção de **CI** (sem `#[ignore]`):
`ledger_spill_adds_a_pointer_when_the_output_exceeds_the_threshold`,
`ledger_without_spill_path_has_no_pointer`, `large_output_is_spilled_with_a_pointer`,
`small_output_is_not_spilled`.

## Decisão (escrita)

O *default* fica **on** — o `Ledger` é o mecanismo unificado de truncagem, substituindo as
truncagens ad-hoc. Não é uma capacidade nova: é a mesma truncagem de antes, mas com head (o modelo
vê o **início** do output, não só a cauda) e com *spill* (o output inteiro é recuperável). O risco
é zero para outputs pequenos (bytes idênticos) e o ganho é a recuperação dos grandes.

## Limites (o que este A/B não diz)

- **Não mede latência de escrita:** o *spill* é best-effort (se a escrita falhar, o head/tail
  mantém-se sem ponteiro). O proxy mede o teto de bytes e o ponteiro, não a latência de I/O real.
- **O spill é por comando:** cada comando grande gera uma página. A rotação de páginas fica
  deliberadamente de fora (o projeto nunca apaga automaticamente; ver E01-T07).
- **O ponteiro é relativo:** `.katu/spill/<id>.<stream>` é relativo à raiz do workspace. O modelo
  precisa de resolver o caminho completo (o `read` já o faz).
