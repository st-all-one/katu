# ADR 0002 — Ferramentas AI-first: envelope + views + TOON (core por medição)

- **Estado:** aceite (o *formato ao modelo* é substituído por [ADR 0005](0005-formato-colunar-d39.md))
- **Data:** 2026-09-29
- **Decisões fundacionais:** DF12 (ferramentas AI-first), DF5 (evidência), DF6 (porta `Memory`),
  G3/G6 (superfície mínima, otimizado para tokens)
- **Épicos:** E06 (tools), E09 (contexto/prime), E15/E18 (medição)

## Contexto

O debate sobre capacidades (2026-09-29) partiu de um problema observável: portar ferramentas
humanas (`cat`/`ls`/`grep`) para o agente **custa tokens com ruído** (licenças, ANSI, `drwxr-xr-x`,
timestamps) e **não dá *affordance* de decisão** — depois de `cat`, o modelo não sabe para onde ir.
O custo não é a leitura; é a **decisão errada** depois de ler, e as **idas e voltas** entre tools.

O katu já exige o resultado disto em §18 ("só o delta chega ao modelo") e G6 ("otimizado para
tokens"), mas faltava a **forma**: um contrato de saída comum, views e um formato ao modelo. O
knudge já resolveu o análogo com o **TOON** (spec `TOON`, D74/D75/D166): subconjunto documentado,
canónico na emissão, sem `null`, vazios omitidos.

## Decisão

1. **Envelope tipado único** (`ToolReport`: `kind`/`id`/`hash`/`data`/`page`/`next`/`cost`) e tools
   **ortogonais** (`read`/`write`/`edit`/`move`/`trash`/`bash`/`grep`/`find`/`ls`/`plan`) — nunca um
   `fs_op(mode=…)`. `ToolOutcome` continua o eixo de **estado**; o envelope é o **payload**.
2. `read` expõe **views** (`outline`/`summary`/`symbol`/`diff`/`full`); `grep`/`find` devolvem
   **hits semânticos** (símbolo + tipo de linha + informação negativa).
3. **Formato ao modelo = subconjunto TOON**, precedido de um **prime compacto**; **JSON** é a
   alternativa de máquina (`format=json`/`--json`).
4. O **core** (índice, cache, syscalls) **só** se otimiza onde o profiler apontar (adoptar-ou-
   reverter, E18); começa em `std::fs` + cache L1 por `path+fingerprint`. `move` entra na família de
   **Escrita** (§1.1 #4).

## Alternatives considered

1. **JSON como formato ao modelo.** Rejeitada: `{}`/`""`/`,` são tokens gastos sem informação; o
   TOON é ~30–50 % mais curto em dados aninhados e já é o contrato do knudge. O JSON fica como
   alternativa de máquina (parseável por outras ferramentas), não como default.
2. **Uma tool única `fs_op(mode=…)`.** Rejeitada: o modelo decide melhor com ferramentas
   **ortogonais e nomeadas**; um `mode` esconde a semântica e dificulta o gate de política por
   operação (E06-T08). Contrato único de **resposta**, ferramentas **distintas**.
3. **Manter a saída humana (`cat`/`ls -la`).** Rejeitada: sem *affordance*, com ruído, não
   determinística (`ls -la` muda por mtime/usuário). Viola G6 e G8.
4. **Construir índice/cache/LSP/syscalls from-zero.** Rejeitada: é a armadilha da "plataforma antes
   do motivo" (arag §20); o plano manda **medir primeiro**. Só entra com A/B e artefacto.
5. **Estrutura por tree-sitter no MVP.** Rejeitada por agora: uma gramática por linguagem (C,
   `unsafe` alheio) é superfície grande; **heurística leve Rust-first** primeiro, tree-sitter gated
   por medição.
6. **Embeddings para busca de ficheiros.** Rejeitada: semântica é a **memória** (knudge); a busca
   de código é **lexical** (ripgrep) — resolve ~80 % dos casos, é determinística e cabe na firewall
   LLM-free.
7. **`move` só via `bash mv`.** Rejeitada: perde atomicidade e invalidação de índice/cache, e a
   política não vê a operação como primitiva. `move` entra como tool (atómica).

## Consequências

- **Positivas:** menos **tool calls por tarefa** (a métrica que importa), não só menos tokens por
  call; saída determinística (cache/prompt estável); IDs estáveis; `next` reduz loops de
  exploração; superfície fechada preservada.
- **Negativas / dívida:** o modelo tem de ser **ensinado** (prime); a spec TOON é **duplicada** em
  `katu-core::toon` (não há crate partilhado — firewall); `move` renumera §1.1 (referências
  atualizadas); envelope "rico" adiciona tokens por chamada (compensa-se cortando chamadas).
- **Travas:** `katu_core::toon` (golden/proptest de determinismo), `E06-T01` (registry fechado),
  `E06-T03` (views/envelope), `E06-T05` (hits semânticos), `E15` (tool calls/tarefa medidos).
