# ADR 0023 — Contexto do projeto (`AGENTS.md`/skills) e duas IAs (embedding + execução)

- **Estado:** aceite
- **Data:** 2026-10-09
- **Decisões fundacionais:** DF2 (política sem I/O), DF4 (fail-closed), DF5 (nunca inventar),
  DF12 (prime determinístico)
- **Épicos:** E20 (T13, T17)
- **Relaciona:** [`SURFACE_IMPLEMENTATION.md`](../../SURFACE_IMPLEMENTATION.md),
  [`docs/CLI_TUI_SURFACE.md`](../CLI_TUI_SURFACE.md), [ADR 0019](0019-superficie-cli-tui-v2.md),
  [ADR 0020](0020-configuracao-global-local.md)

## Contexto

Duas lacunas fecham a onda S4. (1) O `AGENTS.md` da raiz é a **fonte de verdade máxima** do
projeto, mas não entrava no contexto do turno; e as skills `.agents/skill{,s}/*/SKILL.md` não eram
descobertas nem oferecidas. (2) A operação exige **duas IAs** — a de execução (provider) e a de
embeddings — mas a segunda não estava ligada à configuração do katu: `embeddings.*` existia em
`katu.toml` e não era aplicado, e o knudge caía no seu endpoint default (`127.0.0.1:8889`).

## Decisão

1. **`AGENTS.md` no topo do prompt de sistema.** O runtime lê `<raiz>/AGENTS.md` no arranque
   (fail-open: ausência ou bytes inválidos → nada) e injeta-o **antes** da instrução de sistema e
   do prime, com um cabeçalho explícito (`# AGENTS.md (fonte de verdade do projeto)`). É a
   prioridade máxima: em conflito, vence o `AGENTS.md`.
2. **Skills descobertas e oferecidas por catálogo.** O runtime varre `.agents/skill/*/SKILL.md` e
   `.agents/skills/*/SKILL.md` (frontmatter `name`/`description`; sem descrição → não carregada;
   primeiro nome vence) e injeta **nome, descrição e caminho** no prompt de sistema. O modelo lê o
   `SKILL.md` com a tool `read` quando a tarefa o pedir; `/skill:<nome>` **força** o carregamento
   (o conteúdo vira objetivo do turno). Ausência de `.agents/` é fail-open.
3. **Duas IAs, a de embeddings externa e plugável.** A IA de execução é o provider; a de embeddings
   é um **serviço externo** por `embeddings.url` (`http://host:porta/v1`) + `embeddings.model`. A
   projeção para o knudge é feita **em runtime**, sobre uma cópia da config efetiva do knudge
   (`embeddings.enabled`/`provider`/`endpoint`/`model`), sem escrever ficheiros e sem alterar o
   submódulo. `url` ausente/vazia → `embeddings.enabled=false` (**off**; nunca se inventa endpoint,
   DF5). `embeddings.command` fica **reservado** (não lança sozinho o serviço).
4. **`thinking` por omissão ligado a `run`/`tui`.** `provider`/`model`/`base`/`thinking` da config
   alimentam os comandos; em `tui`, o default é aplicado como **controlo logado** no arranque
   (auditável e sobrevive a *resume*). Precedência: flags > `--params` > config (projeto > global) >
   default do comando.

## Alternatives considered

1. **Escrever a config do knudge em disco (`.katu/knowledge/config.toml`) a partir do `katu.toml`.**
   Rejeitada: efeito colateral na abertura da memória e duas fontes de verdade em ficheiro.
2. **Alterar o submódulo do knudge (um `config_override` no builder).** Rejeitada: o knudge é um
   submódulo pinado (`v0.5.2`), fora deste workspace; a projeção em memória obtém o mesmo resultado
   sem tocar nele.
3. **Injetar as skills completas no contexto.** Rejeitada: incha o prompt e gasta orçamento em
   instruções que a tarefa não pede; o catálogo + `read` sob demanda é o modelo do `pi`.
4. **Lançar o `llama-server` pelo `embeddings.command`.** Adiada: exige *probe* de saúde e um lock
   de processo para não multiplicar servidores; `command` fica reservado e visível em `memo doctor`.
5. **Aceitar o endpoint default do knudge (`8889`) quando o katu não configura embeddings.**
   Rejeitada: violaria o “off” e inventaria um endpoint — a segunda IA só existe quando configurada.

## Consequências

- O contexto do turno passa a incluir `AGENTS.md` e o catálogo de skills, sem alterar o `prime`
  (que se mantém byte-idêntico).
- Sem `embeddings.url`, `memo drain --digest` é um no-op `enabled=false` (mudança face ao default
  do knudge); a segunda IA liga-se explicitamente em `katu.toml`.
- `memo doctor` publica o estado dos embeddings (`enabled`/`url`/`model`/`command`).
