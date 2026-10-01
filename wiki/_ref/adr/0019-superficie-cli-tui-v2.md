# ADR 0019 — Superfície v2 do CLI e da TUI

- **Estado:** aceite
- **Data:** 2026-10-09
- **Decisões fundacionais:** DF4 (fail-closed), DF7 (superfície contida), G7 (duas superfícies)
- **Épicos:** E20 (T00), E01 (CLI), E10 (TUI), E14 (governança)
- **Relaciona:** [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md),
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md),
  [`crates/knudge/wiki/specs/cli.md`](../../../crates/knudge/wiki/specs/cli.md)

## Contexto

A superfície atual cresceu por acrescento: verbos `version`/`doctor`/`memory`/`sessions`/`recall`/
`remember`/`run`/`tui`, `--json` **global** e atalhos de tecla fixos na TUI. O contrato do `kd`
(knudge) provou um recorte mais disciplinado — verbos exclusivos, `prime`/`help`, posicional =
**conteúdo**, `--params` universal e envelope estável. O katu precisa do mesmo rigor para servir
tanto a IA (`prime`) como o humano (`help`), sem inferência.

## Decisão

1. **Verbos exclusivos:** `prime`, `upgrade`, `config`, `memo`, `run`, `tui` (+ `help`).
   `version` deixa de ser verbo (`--version` é convenção do `clap`).
2. **Posicional = body**, com stdin/heredoc/pipe: `-` lê stdin; ausente + stdin não-TTY lê stdin;
   ausente + TTY → `invalid_input`.
3. **`prime` estático** (byte-idêntico por versão) e **um `prime` por grupo**; `help` embutido por
   verbo/grupo.
4. **`memo` = consulta/visão geral**, espelhando o `kd` (`ask`/`knowledge`/`doctor`/`sessions`/
   `drain`); **sem escrita** (`remember`/`write`/`forget` saem da superfície).
5. **`--params` XOR flags**: ou `--params` (todas as chaves), ou flags explícitas; ambos → exit 2.
   `--batch` processa JSONL.
6. **`--json` por comando** (não global), presente onde há dados; `--log-level` global com default
   **`quiet`**; **`--quiet` não existe**.
7. **`--init` é flag de topo** (não verbo): `katu --init [--git-excluded|--git-tracked]` faz
   bootstrap e sai; `katu` sem verbo abre a TUI; **sem TTY falha fechado**.
8. **`run` devolve id + exit da rodada** (envelope `session` + `round_exit`; `exit` do processo =
   o da rodada).

## Alternatives considered

1. **Manter `--json` global.** Rejeitada: `katu --json` sem verbo não faz sentido (a TUI não emite
   JSON); o envelope pertence a quem produz dados.
2. **Manter `version` como verbo.** Rejeitada: `--version` é convenção; verbos são ações.
3. **`memo` com escrita (`remember`/`write`).** Rejeitada: a escrita de memória é do **agente**
   (tools no loop) e do `kd`; o `memo` é leitura/visão geral.
4. **`--params` + flags em simultâneo.** Rejeitada: ambiguidade e inferência; exclusivo é
   explícito (I1).
5. **`--quiet` além de `--log-level`.** Rejeitada: dois caminhos para o mesmo efeito;
   `--log-level=quiet` basta.
6. **`katu` sem verbo = `help` (como o `kd`).** Rejeitada: o katu é uma UI; abrir a TUI é o gesto
   primário (e o `--init` cobre o bootstrap não-interativo).

## Consequências

- **Positivas:** superfície mínima e previsível; IA e humano com `prime`/`help`; scripts com
  stdin/`--params`/`--batch`; nenhuma inferência.
- **Negativas / dívida:** quebra de retrocompatibilidade (sem aliases); a suíte de CLI (topo) muda;
  exige bootstrap do `.katu/` no arranque.
- **Travas:** testes por verbo; `prime` byte-idêntico; `check-surface`; `docs/CLI_TUI_SURFACE.md`
  atualizado; [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md) como épico de controlo.
