# PROTOCOL — composição do prompt (Q-20)

**Pergunta.** De que é feito o prompt que o katu envia ao modelo, e quanto custa cada parte?

**Porque existe.** O [`REPORT.md`](../REPORT.md) §121 admitia que a composição dos 3265 tokens
**não** estava medida: sabia-se o total (`input_tokens`), não a origem. Sem isso, a frente F2
("atacar o prompt") otimizava às cegas — e nada travava uma regressão.

**Método.** `xtask gate:prompt` reconstrói a composição a partir das **funções de produção**
(nunca de uma cópia):

| parte | fonte |
|---|---|
| `AGENTS.md` | ficheiro da raiz, como o `Runtime` o lê |
| catálogo de skills | `katu_core::skill::{discover, catalog}` sobre `.agents/skill{,s}/*/SKILL.md` |
| prime | `katu_core::context::prime()` |
| tools (wire JSON) | `katu_tools::schema::{tool_defs, wire_json}` — a forma `{"type":"function",…}` do `openai/encode.rs` |

Os tokens são estimados com o rácio **medido** de Q-01
(`katu_core::context::tokens_from_bytes`, 3,631 B/token), o mesmo que o orçamento do kernel usa.

**Comandos.**

```sh
cargo run -p xtask -- gate:prompt            # verifica os orçamentos (corre no `check`)
cargo run -p xtask -- gate:prompt --write    # regrava `raw.json` (determinístico)
```

**Orçamentos** (teto; exceder falha o gate): `AGENTS.md` 1600 B · catálogo de skills 1300 B ·
prime 1400 B · tools 5100 B · `system` (AGENTS+prime+skills) 2200 tok.

## Resultado

| parte | antes (§1.5) | agora | Δ |
|---|---|---|---|
| `tools` (wire JSON) | 5013 B / 1381 tok | 5013 B / 1381 tok | — (contrato do endpoint) |
| catálogo de skills | 4828 B / 1330 tok | **1211 B / 334 tok** | **−73,9 %** (Q-05) |
| `AGENTS.md` | 1591 B / 438 tok | **1128 B / 311 tok** | **−29,1 %** (Q-19) |
| prime | 1340 B / 369 tok | 1340 B / 370 tok | — |
| **`system`** | 7759 B / 2137 tok | **3679 B / 1014 tok** | **−51,4 %** |
| **prompt (system + tools)** | ~3518 tok | **~2394 tok** | **−30,9 %** |

Os dois alvos controláveis caíram: o catálogo de skills por **descrição de uma linha + caminho
relativo** (Q-05) e o `AGENTS.md` por **condensação determinística** (Q-19). Os `tools` (38 % do
prompt) não descem — são o contrato JSON do endpoint (Q-18 decide se se envia um subconjunto).

**Nota.** A quente, o prefix-cache do servidor paga o prompt **uma vez** (`cached=3254/3265`): o
ganho não é latência por turno, é o **arranque a frio** (~40 s), o custo em provider remoto e o
*headroom* de janela.

**Rejeitado (com número).** Um cache em `.katu/context/` com validação por *fingerprint* (Q-19):
a condensação é uma passagem linear sobre ~1,6 KB (classe do `toon::colunar`: 89 µs em dev,
`bench/e18/atomics`), enquanto `fs.write` com `sync_all` custa **25,8 ms** (`bench/e18/raw.json`).
~300× o custo do cálculo para introduzir risco de staleness — rejeitado; o cálculo é em memória.

## Reprodutibilidade

`raw.json` é determinístico (mesma fonte → mesmos bytes; a ordem do catálogo é canônica e o
objetivo é vazio na medição). `--write` regrava-o; o `check` só verifica os orçamentos.
