# ADR 0005 — Formato ao modelo: colunar D39 (header autodescritivo, `\x1f`)

- **Estado:** aceite (emendado por [ADR 0006](0006-toon-colunar-v3.md))
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF12 (ferramentas AI-first), DF5 (evidência), G3/G6 (superfície
  mínima, otimizado para tokens)
- **Épicos:** E06-T12 (formato AI-first), E09 (prime), E15/E18 (medição)
- **Substitui parcialmente:** [ADR 0002](0002-ferramentas-ai-first.md) §Decisão 3 (o *formato ao
  modelo*); o envelope tipado, as views e o JSON de máquina mantêm-se.

## Contexto

A ADR 0002 fixou "formato ao modelo = subconjunto **TOON**" e o `katu-core::toon` absorveu a spec
`TOON` do knudge (`wiki/specs/TOON.md`, D74/D75) — o contrato de **bytes do frontmatter**: aninhado,
indentado, `key: value`, vazios omitidos. Esse formato é o *byte-canónico do corpus*, não o
contrato de **consumo por LLM**.

O knudge resolve o consumo de agente com um contrato diferente, o **D39** (`kd ask`/`recall`):
`id|statement|score|why`, uma linha por registo, **schema declarado uma vez**, colunas posicionais,
vocabulário fechado (`why`), precisão fixa e revelação progressiva. É denso *e* de maior qualidade
para o modelo: menos tokens por facto, posições previsíveis, sem repetição de chaves.

O `toon` atual repete as chaves em **cada** item de lista (`- id: …\n  kind: …`), que é exatamente
o custo que o D39 elimina. Medido nos relatórios reais (`read.summary`, `search.grep`,
`memory.recall`), as listas dominam os bytes.

## Decisão

O **formato ao modelo** passa a ser **colunar D39**: um *stream* plano de **tabelas**, sem
aninhamento e sem células-lista. Cada tabela é um **header autodescritivo** seguido de N **linhas**
homogéneas.

### 1. Bytes (contrato)

- **`\x1e`** (RS): prefixo da linha de **header**. Formato:
  `\x1e<nome>\x1f<col1>\x1f<col2>…\n`.
- **`\x1f`** (US): separador de **células** dentro de uma linha.
- **`\n`** (LF): termina uma linha. Sem CR. Sem BOM.
- Uma linha que **não** começa por `\x1e` é uma **linha de dados** da tabela do header anterior,
  com exatamente `len(cols)` células.
- **Células são escalares.** `\x1f`, `\x1e` e quebras dentro de conteúdo são **sanitizados** para
  espaço (o emissor nunca os emite crus). Código preserva indentação (tab/espaço), porque só `\x1f`
  é sanitizado.
- **Ausente = célula vazia** (`a\x1f\x1fc`). Não existe `null`.
- Tabela **sem linhas é omitida** (não se emite o header).

### 2. Domínios fechados no header

Uma coluna pode declarar o domínio fechado dos seus valores:
`nome{opt1,opt2,…}`. Ex.: `why{file,anchor,stars,semantic,recent,universal}`. Sem `{}`, o domínio
é livre. Isto põe a *opção fixa* junto do ponto de uso e reduz a dependência do prime.

### 3. Ordem = estabilidade decrescente

- **Blocos:** `r` (envelope universal) → `summary` (escalares da tool) → tabelas de detalhe (ordem
  fixa por tool) → `next` (ações) → diagnóstico.
- **Colunas:** chaves/ids → enums → estruturais (path, lang, ranges) → numéricos → **texto livre
  por último** (`statement`, `preview`, `text`).
- **Linhas:** ordem determinística por chave estável (id, path, line); desempate por id. Resultados
  ranqueados levam uma coluna `rank` (1..n) e ordenam por score desc, desempate id asc.

Dados previsíveis caem sempre nas mesmas posições de uma tabela do mesmo nome.

### 4. Quantização fixa

- booleano → `0`/`1`; contagens/bytes/ms/tokens → inteiro; sem `float`.
- score/confiança → inteiro **permilagem** `0..=1000`.
- sequências de aridade fixa **viram colunas** (`[start,end]` → `start,end`; nunca listas).
- listas variáveis **viram tabela** (ex.: `imports`, `next`).

### 5. Envelope universal `r`

Colunas fixas (sempre presentes, célula vazia quando ausente):
`kind, id, hash, cursor, total, trunc, bytes, ms, tok`.

### 6. Código e conteúdo literal

Código nunca é célula de tabela de detalhe: vai numa tabela `lines` com colunas `n` (número de
linha, 1-based) e `text` (a linha crua, indentação preservada). Isto unifica `read.full`/`range`/
`symbol` com o resto e mantém o paginamento (`cursor`/`total`) no envelope.

### 7. JSON inalterado

`format=json`/`--json` continua a serializar o **mesmo** `ToolReport` via `Serialize`. O TOON é o
default ao modelo; o JSON é a alternativa de máquina.

### 8. Medição (dev-only)

- Harness **dev-only** (fora de `katu-core`/`katu-policy`/`katu-tools`, pela firewall LLM-free) com
  um **tokenizer real** e um **micro-bench** sobre um corpus fixo de relatórios.
- Publica-se apenas com base DF5 (`bench/published.toml`), como qualquer outro número.

## Alternatives considered

1. **Manter o TOON aninhado (ADR 0002).** Rejeitada: repete chaves por item; é o byte-contract do
   frontmatter (corpus), não o contrato de consumo do agente (D39).
2. **Delimitador `|` (D39 literal).** Rejeitada: colide com código Rust (`|x|`), shell e markdown, e
   exigiria escape/sanitização que inflaciona. `\x1f` é raro no conteúdo e não colide com prosa.
3. **Delimitador `\t`.** Rejeitada: aparece na indentação de código (que queremos preservar) e o
   tokenizador não o trata melhor que `\x1f`.
4. **Header implícito (só prime).** Rejeitada: posições ficam dependentes de contexto externo; um
   header por tabela é autodescritivo e barato (uma linha por bloco).
5. **Manter aninhamento para listas de mapas irregulares.** Rejeitada: destrói a previsibilidade
   posicional. Listas irregulares achatam-se em tabelas com coluna de agrupamento.
6. **Células-lista com sub-separador.** Rejeitada: complexidade de parsing sem ganho; cada lista
   vira tabela (colunar puro).
7. **Floats com precisão variável.** Rejeitada: ruído e posição imprevisível; quantização inteira.

## Consequências

- **Positivas:** menos tokens por facto (sem chaves repetidas); posições estáveis por tabela; enums
  no header; determinismo byte-a-byte (cache/prompt estável); `cost.bytes`/`tok` descem.
- **Negativas / dívida:** quebra de contrato — todos os goldens e views migram; exige **prime** para
  ensinar `\x1e`/`\x1f`; `\x1f` é invisível em inspeção humana (debug usa o JSON).
- **Travas:** `katu_core::toon` (golden + proptest de determinismo e sanitização), `E06-T12`
  (golden do formato), `E15` (tool calls/tarefa) e o micro-bench (densidade medida, DF5).
