# Architecture Decision Records (ADRs)

> **E14-T01.** Uma decisão arquitetural sem o que **venceu** convida a re-litigá-la. Cada ADR
> regista o contexto, a decisão e — obrigatoriamente — as **alternativas consideradas**.

## Convenções

- **Ficheiro:** `NNNN-titulo-em-kebab.md`, com `NNNN` zero-padded (`0001`, `0002`, …). Números
  **nunca** se reutilizam.
- **Secções obrigatórias:** `## Contexto`, `## Decisão`, `## Alternativas consideradas`,
  `## Consequências`.
- **Imutáveis:** uma ADR **não** se edita para outra decisão. Substitui-se por uma nova e liga-se
  a ambas (`Supersedes` / `Superseded by`).
- **Verificação:** `xtask check-docs` (em `make check`) falha se faltar `## Alternatives considered`.
- **Relação com o plano:** as decisões **fundacionais** vivem em
  [`plan/01-decisoes-fundacionais.md`](../../plan/01-decisoes-fundacionais.md) (`DFxx`); a ADR
  regista a **decisão de fase** que as materializa (ex.: um gate que passa). Um facto, um lar.

## Índice

| ADR | Título | Estado |
|---|---|---|
| [0001](0001-mvk-gate-aprovado.md) | MVK aprovado — o loop possuído (DF1) torna-se compromisso | aceite |
| [0002](0002-ferramentas-ai-first.md) | Ferramentas AI-first: envelope + views + TOON (core por medição) | aceite |
| [0003](0003-vocabulario-v2-contencao.md) | Vocabulário de política v2: leitura sensível e acesso fora do workspace | aceite |
| [0004](0004-sem-ffi-kill-grupo-e17.md) | Sem FFI no MVP: kill do grupo de processos fica para a jail (E17) | substituída por 0018 |
| [0005](0005-formato-colunar-d39.md) | Formato ao modelo: colunar D39 (header autodescritivo, `\x1f`) | emendado por 0006 |
| [0006](0006-toon-colunar-v3.md) | TOON colunar v3: sem headers, blocos literais e aliases de sessão | emendado por 0007 |
| [0007](0007-projecoes-model-facing.md) | Projeções model-facing, digest `m`, catálogo de tools e emissor direto | aceite |
| [0008](0008-sessoes-identidade-e-retomada.md) | Sessões: identidade, vinculação ao projeto, snapshot e retomada | aceite |
| [0009](0009-auditoria-densa.md) | Auditoria densa e pesquisável (`.katu/audit`, completa sob compactação) | aceite |
| [0010](0010-porta-memory-e-substituibilidade.md) | Memória de primeira classe e substituível (porta `Memory` + adaptador knudge) | aceite |
| [0011](0011-porta-provider-e-builtin-opencode.md) | Porta `Provider` e built-in `opencode go/zen` sobre transporte bloqueante | aceite |
| [0012](0012-catalogo-dialetos-e-providers-declarativos.md) | Catálogo de dialetos e providers declarativos | aceite |
| [0013](0013-cache-de-prefixo-e-compressao-de-pedido.md) | Cache de prefixo por modelo e compressão de pedido (medida) | aceite |
| [0014](0014-orcamento-de-latencia-do-provider.md) | Orçamento de latência do provider: gate offline determinístico | aceite |
| [0015](0015-loop-de-turnos-e-roteador.md) | Loop de turnos e roteador de tool calls | aceite |
| [0016](0016-politica-de-memoria-e-unsafe.md) | Política de memória e `unsafe` | aceite |
| [0017](0017-recursos-e-runtime-minimo.md) | Política de recursos e runtime mínimo | aceite |
| [0018](0018-kill-do-grupo-com-unsafe-unico.md) | Kill do grupo de processos com um único `unsafe` | aceite |
| [0019](0019-superficie-cli-tui-v2.md) | Superfície v2 do CLI e da TUI | aceite |
| [0020](0020-configuracao-global-local.md) | Configuração global/local e `--params` | aceite |
| [0021](0021-layout-katu-e-versionamento.md) | Layout central do `.katu/` e versionamento | aceite |
| [0022](0022-modo-plano-e-deny-write-outside.md) | Modo de planeamento (`/plan`) e `deny_write_outside` (vocabulário v3) | aceite |
| [0023](0023-contexto-e-duas-ias.md) | Contexto do projeto (`AGENTS.md`/skills) e duas IAs (embedding + execução) | aceite |
| [0024](0024-durabilidade-do-log.md) | Durabilidade do log: `fsync` por evento ou por turno (opt-in) | aceite |

## Template

```markdown
# ADR NNNN — <título>

- **Estado:** proposto | aceite | substituído por ADR NNNN
- **Data:** AAAA-MM-DD
- **Decisões fundacionais:** DFxx, …
- **Épicos:** Enn, …

## Contexto

O que obrigou a decidir; a evidência disponível (com artefacto, DF5).

## Decisão

O que fica decidido, em uma ou duas frases verificáveis.

## Alternativas consideradas

1. **<alternativa>.** Porque foi rejeitada.
2. …

## Consequências

- **Positivas:** …
- **Negativas / dívida:** …
- **Travas:** testes/gates que impedem a regressão.
```
