# ADR 0007 — Projeções model-facing, digest `m`, catálogo de tools e emissor direto

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF5 (evidência), DF9 (diagnóstico), DF12 (ferramentas AI-first)
- **Épicos:** E06-T12, E09-T07, E18 (medição), E19 (instrumentação)
- **Emenda:** [ADR 0006](0006-toon-colunar-v3.md) (mantém o formato colunar v3; fecha as costuras
  model-facing que ficaram fora)

## Contexto

Depois do v3 (ADR 0006), o formato dos **relatórios de tools** era colunar, mas restavam costuras
model-facing fora do registo e sem projeção canónica:

- o **digest** da compactação (E09-T07) usava `format!("{outcome:?}")` — `Debug` de Rust, instável
  e verboso — e linhas soltas, sem esquema;
- `ToolOutcome`, `Error` e `VerificationReport` não tinham projeção para `Value`;
- o **catálogo de tools** no prime era uma lista escrita à mão, com risco de drift face a
  `katu_tools::schema::SCHEMAS`;
- o **emissor** alocava uma `String` por célula (`Cell::render`).

O dono privilegiou **um prime claro e sem drift** sobre mais compacto (não se aplica a frente G).

## Decisão

1. **Projeções model-facing** (`katu-core/src/model.rs`): `ToolOutcome::to_value`,
   `Error::to_value` e `VerificationReport::to_value`. Escalares no topo viram secção `k`; listas
   viram tabelas do registo (`checks`). Domínios fechados vêm do registo — a mesma fonte do prime.
2. **Digest = tabela `m`** (`kind{user,assistant,tool_call,tool_result}`, `text`), **sem `Debug`** e
   **sem id no *stream*** (o id continua no log; `recover`/`replacements` intactos). O orçamento é
   aplicado por comprimento de bytes exato (a sanitização preserva comprimento).
3. **Catálogo de tools** = `katu_tools::schema::catalog()` (tabela `tool`; domínios fechados inline,
   `?` = opcional), injetado por `katu_core::context::prime_with_catalog`. **Camadas respeitadas**:
   o core não importa as tools; o chamador (seam E12) passa a string.
4. **Auto-validação do registo** (`toon::schema::validate`) e **gate** `check-schemas` que cobre o
   linter de tools **e** o registo/catálogo (anti-drift).
5. **Emissor direto**: `emit_rows`/`emit_cell` escrevem no buffer (sem `render()`/`String` por
   célula) e com *hint* de capacidade. Instrumentação estruturada nova: `toon.project`,
   `toon.emit`, `model.project`, `context.digest`, `schema.catalog`.
6. **Qualidade a custo mínimo:** `score` do recall em **permilagem** (`0..=1000`); `ev` (âncora) é
   path para efeitos de alias.

## Alternatives considered

1. **Manter o digest com `Debug`.** Rejeitada: instável, vaza nomes internos e não é reconstruível
   pelo registo.
2. **Digest com coluna `id`.** Rejeitada por medição: o id content-addressed (`m_<16hex>`) tokeniza
   muito mal; removê-lo do *stream* corta **-36 %**. O id fica no log para `recover`.
3. **Catálogo de tools derivado no core.** Rejeitada: violaria o firewall (`katu-core` não pode
   importar `katu-tools`). A injeção por parâmetro mantém a fronteira.
4. **Catálogo inline no prime (`prime` de topo).** Rejeitada: as tools vivem num crate de cima; a
   lista à mão era a fonte de drift.
5. **Tabela `d` para outcomes/erros.** Rejeitada: uma entidade única é mais densa como `k` (chave
   explícita) do que como tabela de uma linha.
6. **Emissor com `&str`/`Cow` (células emprestadas).** Adiada: refactor de tipos em toda a árvore
   por ganho sublinear neste corpus; a escrita direta captura o essencial.

## Consequências

- **A/B (dev-only, `xtask --features tokenizer -- bench-toon`):**
  - **digest:** 136 vs 211 tokens (**-36 %** vs o formato antigo com `Debug`+id);
  - **catálogo:** prime 373 → 464 tokens (**+91**), custo explícito da **clareza/anti-drift** (o
    dono preferiu claridade a compactação);
  - **emissor:** `emit` ~1,16–1,21× mais rápido (perfil dev; escrito direto sem alocação por célula);
  - **relatórios:** v3 **-18 %** vs JSON no corpus alinhado ao registo.
- **Instrumentação:** **68 ids** no catálogo; os novos spans medem `toon.project`/`toon.emit`/
  `model.project`/`context.digest`/`schema.catalog` (duração no *drop*).
- **Travas:** testes de conformidade do registo, projeções model-facing, catálogo e digest;
  `check-schemas` no CI.
- **Negativas:** o catálogo aumenta o prime (+91 tokens, amortizado 1× por sessão); a injeção do
  catálogo no caminho real depende do seam E12 (mensagens ao provider), ainda por construir.
