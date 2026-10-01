# Q-01 — rácio bytes/token medido

**Objetivo.** Substituir a estimativa `bytes/4` de [`context`](../../../crates/katu-core/src/context.rs)
por um rácio **medido** com o tokenizer do modelo, para o orçamento de contexto
([`ContextBudget`](../../../crates/katu-core/src/context.rs)) deixar de subestimar os tokens.

**Porque importa.** `bytes/4` **subestima** os tokens de texto misto (prosa PT + código + TOON +
caminhos): o orçamento `raw_min` admite mais texto do que julga caber, e `needs_compaction`
dispara tarde. O erro é determinístico e mensurável.

## Protocolo

1. Corpus **fixo** no *heredoc* `CORPUS` de [`measure.sh`](measure.sh) (2124 bytes): prime PT,
   descrições de tools em inglês, código Rust, um JSON Schema, TOON e caminhos típicos.
2. Tokenizar com o tokenizer do próprio modelo, via `POST /tokenize` do `llama-server`
   (`KATU_TOKENIZE_URL`, por omissão `http://127.0.0.1:8080/tokenize`).
3. Rácio = `bytes / tokens`. Repetir 3× e confirmar determinismo.
4. Publicar em [`bench/published.toml`](../../published.toml) com base `measured` (DF5) e este
   artefacto; o valor usado no código é `BYTES_PER_TOKEN_MILLI`.

```sh
bench/e18/tokens/measure.sh     # 3× → bench/e18/tokens/raw.json
```

## Resultado

| Modelo | Corpus (B) | Tokens | bytes/token | `bytes/4` (B) | desvio |
|---|---|---|---|---|---|
| `qwen2.5-coder-1.5b` (llama.cpp, CPU) | 2124 | 585 | **3.631** | 4.0 | **−9,2 %** |

Três execuções idênticas (ver `runs` em [`raw.json`](raw.json)): determinístico. Em código:
`BYTES_PER_TOKEN_MILLI = 3631`.

## Critério de adoção (E18 §0.3)

O alvo é o **desvio** `estimado` vs `tokenizer_real`. `bytes/4` errava 9,2 %; o rácio medido leva o
desvio a ≈0 % neste corpus (≫ 20 % de melhoria no alvo). **Adotado.**

## Limites (honestidade)

- O rácio é **do modelo**: outro tokenizer (outro modelo) muda-o. O desvio residual fica observável
  em `provider.request` (`input_tokens` reportados pelo endpoint, Q-01).
- O corpus é fixo mas pequeno (2,1 KB): mede texto misto, não garante o mesmo rácio em código denso
  ou binário. Um tokenizer exato só entra se um A/B provar *misbudget* acima do limiar (Q-01).
- `tokens_from_bytes` continua a ser uma **estimativa**; não substitui a contagem do provider. O
  que muda é a constante: deixou de ser um chute fixo e passou a ter artefacto.
