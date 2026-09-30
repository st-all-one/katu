# ADR 0006 — TOON colunar v3: sem headers, blocos literais e aliases de sessão

- **Estado:** aceite (emendado por [ADR 0007](0007-projecoes-model-facing.md))
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF12 (ferramentas AI-first), DF5 (evidência), G6 (otimizado para tokens)
- **Épicos:** E06-T12, E09 (prime), E15/E18 (medição)
- **Emenda:** [ADR 0005](0005-formato-colunar-d39.md) (mantém colunar/D39; muda o envelope de bytes)

## Contexto

O formato v2 (ADR 0005) punha um **header por tabela** no *stream* (nomes de coluna + domínios). O
micro-bench dev-only (`xtask --features tokenizer -- bench-toon`, `cl100k_base`) mostrou que esse
custo fixo **domina os relatórios pequenos**: `exec.run` era **+53%** vs JSON e `read.diff` **+5%**,
apesar de ganhar **-15%** em relatórios de registos. O objetivo (densidade **sem perder qualidade**)
exige tirar o header repetido do caminho quente e recuperar a codificação de conteúdo textual.

## Decisão

1. **Sem headers no *stream*.** O **prime** passa a ser o **registo de esquema**: por secção, o modo
   (tabela de linhas / bloco literal) e as colunas por ordem, com os domínios fechados. O prime é
   emitido no início de **cada sessão** e sempre que o contexto é remontado **após compactação**
   (`assemble` já o prefixa). Fonte única de verdade: [`toon::schema`](../../crates/katu-core/src/toon/schema.rs)
   — a projeção e o prime leem daqui (sem drift).
2. **Blocos literais** (`\x1dNOME` + linhas cruas, sem `\x1f`) para código/prosa (`text`, `stdout`,
   `stderr`, `hunks.lines`). **Sem numeração de linhas** por defeito: o offset vem do `k` (`start`).
3. **Escalares explícitos** na secção `k` (`k`,`v`): sem `null` e **sem defaults/omissão**.
4. **Nomes de coluna mínimos mas claros** (`ln`, `ty`, `syms`, `desc`, `cur`, `tot`), fixados no
   registo para reduzir *drift* de interpretação.
5. **Aliases de sessão** (`#N` ids, `@N` paths) na secção `sym`, aplicados ao nível do `Value`
   (`ToolReport::to_toon_with`) com **limiar de 3 usos** (a declaração custa a string inteira; 1–2
   usos não pagam).
6. **Qualidade a custo mínimo:** recall com `rank`,`basis` e evidência `ev`; grep com `sym` por hit;
   domínios fechados no registo (`kind`, `ty`, `basis`, `status`, …).

### Bytes (contrato)

- `\x1e` (RS) prefixa uma secção de **linhas**; `\x1d` (GS) prefixa um **bloco literal**; `\x1f` (US)
  separa células; `\n` termina a linha.
- Sem header: as colunas e o modo vêm do prime. Célula vazia = ausente; booleano `0`/`1`; sem floats.

## Alternatives considered

1. **Manter headers por tabela.** Rejeitada: é o custo fixo que domina os relatórios pequenos (medido).
2. **Esquema todo no prime, sem marcador de secção.** Rejeitada: perde robustez a secções opcionais;
   mantém-se um marcador de 1 byte (`\x1e`/`\x1d`).
3. **Uma segunda tabela `sym` global entre chamadas.** Rejeitada: exige estado mutável no caminho de
   render; o `sym` por relatório + prime reconstruível do log é determinístico e local.
4. **Alias eager (1ª utilização).** Rejeitada por medição: registar entities de uso único (12
   símbolos, 24 paths, 8 notas) custa mais do que poupa. Fica **lazy com limiar 3**.
5. **Números de linha nos blocos literais.** Rejeitada: custa `n`+US por linha e quebra a compressão
   do tokenizer; o offset vive no `k`.
6. **Defaults declarados no registo (`*=measured`).** Rejeitada pelo requisito de manter valores
   **explícitos** (evitar drift).

## Consequências

- **Positivas (A/B, `cl100k_base`, corpus representativo):** total **1720 tokens** vs JSON **2177**
  (**-21%**; o v2 era -10%); `read.diff` **186 vs 245 (-24%)** (o v2 era +5%); `exec.run` **82 vs 73
  (+12%)** (o v2 era +53%); `read.summary` **-30%**.
- **Aliases:** no corpus sintético (7 relatórios) são **neutros** (1721 vs 1720) — só há um caminho
  quente (`read.full`/`edit`/`read.diff` sobre o mesmo ficheiro, onde `read.diff` cai a **-30%**).
  O ganho depende da **reutilização por sessão**; medir num log real antes de publicar (DF5).
- **Negativas / dívida:** o modelo tem de ter o prime fresco (injeção no início e após compactação);
  o registo tem de cobrir todas as secções (fonte única + testes de conformidade).
- **Travas:** `katu_core::toon` (golden/proptest de determinismo, sanitização e aliases),
  `E06-T12`, `E15` e o micro-bench.
