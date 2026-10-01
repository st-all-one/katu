# TOON ao modelo — melhorias e medições (v3 / v3.1)

Registo vivo das melhorias de **densidade e qualidade** do formato ao modelo (ADR 0005 → 0006 →
0007). As decisões estão nos ADRs; aqui ficam os **números** e o método, para A/B reprodutível.

## Como medir

```
cargo run -p xtask --features tokenizer -- bench-toon
```

- Dev-only (feature `tokenizer`), **fora** do `make check`; publicar números exige base DF5.
- Tokenizer real `cl100k_base` (`tiktoken-rs`); corpus determinístico com as formas reais das tools,
  **alinhado ao registo** `toon::schema`.
- Base `inferred` para qualquer número aqui; o custo por resposta é `advisory`.

## Densidade por relatório (v3 vs JSON, corpus alinhado)

| Relatório | v3 | JSON | v3−JSON |
|---|---:|---:|---:|
| `read.summary` | 329 | 470 | **−30 %** |
| `read.full` | 74 | 83 | −11 % |
| `edit.patch` | 86 | 78 | +10 % |
| `search.grep` | 753 | 912 | −17 % |
| `memory.recall` | 267 | 316 | −16 % |
| `read.diff` | 186 | 245 | **−24 %** |
| `exec.run` | 82 | 73 | +12 % |
| **TOTAL** | **1777** | **2177** | **−18 %** |

Leitura: ganhos fortes em registos homogéneos; `edit`/`exec` pagam o envelope e poucos campos
(conteúdo textual é recuperado por blocos literais, mas o delta é pequeno). O v2 (`ADR 0005`) era
−10 % no corpus de então; o corpus foi entretanto alinhado ao registo, pelo que v2/v3 não são
diretamente comparáveis a esta tabela.

## A/B das costuras (v3.1, ADR 0007)

| Frente | Antes | Depois | Efeito |
|---|---|---|---|
| Digest de compactação | `Debug` + linhas soltas (`kind id text`) | tabela `m` (`kind`,`text`), sem id no stream | **−36 %** (211 → 136) |
| Catálogo de tools no prime | lista `read/write/…` à mão | tabela `tool` com domínios fechados | **+91** (373 → 464) — clareza/anti-drift |
| Emissor | `String` por célula (`render()`) | escrita direta no buffer | **~1,16–1,21×** (perfil dev) |

`edit.patch` isolado fica **+10 %** vs JSON por causa da **declaração de alias** (`sym` custa a
string inteira); a entidade que reaparece depois (`read.diff`) ganha **−30 %**.

## Decisões adotadas / rejeitadas (por medição)

- **Adotado:** aliases **lazy** com **limiar 3** usos. Eager (1.ª utilização) tinha custo líquido
  negativo em entidades de uso único (12 símbolos, 24 paths, 8 notas).
- **Adotado:** digest **sem** a coluna `id` (content-addressed) — tokeniza muito mal; o id continua
  endereçável no log (`recover`).
- **Adotado:** blocos literais para código/prosa, sem numeração por linha (offset vem do `k`).
- **Rejeitado:** headers por tabela no *stream* (custo fixo dominante nos relatórios pequenos).
- **Mantido por preferência:** prime **claro** (catálogo de tools) mesmo com +91 tokens.

## Instrumentação (para aferir em runtime)

5 ids novos no catálogo (**68 ids** no total): `toon.project`, `toon.emit`, `model.project`,
`context.digest`, `schema.catalog`. Com `--features instrument` e `KATU_INSTRUMENT=1`, cada span
regista a **duração no `drop`** e o digest regista `rows`/`bytes` — medição estruturada, sem texto
livre, e invisível ao modelo.

## Ficheiros

- Decisões: [`docs/adr/0006-toon-colunar-v3.md`](../adr/0006-toon-colunar-v3.md),
  [`docs/adr/0007-projecoes-model-facing.md`](../adr/0007-projecoes-model-facing.md).
- Harness: `xtask/src/toon_bench.rs` (+ `xtask/src/toon_bench/corpus.rs`).
- Formato: `crates/katu-core/src/toon/{schema,project,colunar,aliases}.rs`.
- Projeções model-facing: `crates/katu-core/src/model.rs`.
