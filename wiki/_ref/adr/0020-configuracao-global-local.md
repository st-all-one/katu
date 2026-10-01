# ADR 0020 — Configuração global/local e `--params`

- **Estado:** aceite
- **Data:** 2026-10-09
- **Decisões fundacionais:** DF4 (fail-closed), DF7 (superfície contida), G4 (memória de primeira
  classe)
- **Épicos:** E20 (T08, T09, T18), E01, E12
- **Relaciona:** [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md),
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md),
  [ADR 0019](0019-superficie-cli-tui-v2.md)

## Contexto

A configuração do katu vivia em flags dispersas por comando (`--provider`, `--model`, …) e num
`policy/tiers.toml` que só resolvia a fase → modelo. Faltava um **lar** para os padrões do
utilizador (provider/modelo/thinking, embeddings, comportamento) e uma forma de os sobrepor por
projeto. Ao mesmo tempo, o `kd` provou que `--params` (um objeto que representa **toda** a config do
comando) simplifica scripts — mas coexiste mal com flags explícitas se não houver regra.

## Decisão

1. **Uma config global** por SO: `~/.config/local/katu/katu.toml` (Linux, respeitando
   `XDG_CONFIG_HOME`), `~/Library/Application Support/katu/katu.toml` (macOS),
   `%APPDATA%\katu\katu.toml` (Windows).
2. **Override local** em `<projeto>/.katu/katu.toml`; **precedência projeto > global**, chave a
   chave.
3. **Conjunto fechado de chaves** (`provider`, `model`, `thinking`, `base`, `log_level`,
   `git.versioned`, `memory.persist_in_project`, `behavior.auto_compact`, `recall.default_limit`,
   `embeddings.{url,model,command}`); chave ou valor inválido → `invalid_input` (exit 2), com a
   lista e uma sugestão.
4. **`--params '{…}'`** representa toda a config do comando e é **exclusivo** com as flags
   explícitas (ambos → exit 2, sem inferência); `--params -` lê `stdin`. `--batch <ficheiro|->`
   processa JSONL e **valida tudo antes** de executar (uma linha inválida recusa o lote inteiro).
5. **`init` copia a global 1:1** para o projeto (snapshot, E20-T18): o projeto **não** segue
   mudanças globais futuras; `config set` local edita o snapshot.
6. **Segredos** ficam só na global; o `get`/`list` efetivo nunca os move para o projeto.

## Alternatives considered

1. **Só flags (sem `katu.toml`).** Rejeitada: não há lar para padrões do utilizador nem override
   por projeto; scripts ficam frágeis.
2. **Merge contínuo global→projeto (camada viva).** Rejeitada: o requisito do dono é uma cópia 1:1
   no `init`; um merge vivo tornaria o comportamento dependente de estado externo não versionado.
3. **`--params` + flags em simultâneo (flags vencem).** Rejeitada: ambiguidade e inferência (I1);
   exclusivo é explícito e testável.
4. **`--params` como camada sobre os defaults, sem XOR.** Rejeitada: não se distingue "não
   indicado" de "indicado igual ao default", pelo que a intenção se perde.
5. **Config por variáveis de ambiente.** Rejeitada: não é versionável nem tem conjunto fechado;
   dificulta a auditoria.

## Consequências

- **Positivas:** um lar para padrões; precedência previsível; scripts com `--params`/`--batch`;
  nenhum valor inventado (chave fechada + `deny_unknown_fields`).
- **Negativas / dívida:** `--params` cobre `prime`/`run`/`tui`/`memo ask` e `--batch` cobre
  `prime`/`run`/`memo ask`; `config`/verbos em esqueleto ficam para depois; o `set`/`unset` local
  escreve no snapshot (não segue o global).
- **Travas:** testes de precedência, de chave desconhecida e de XOR; `check-surface`; o uso vive em
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md).
