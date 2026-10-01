# ADR 0022 — Modo de planeamento (`/plan`) e `deny_write_outside`

- **Estado:** aceite
- **Data:** 2026-10-09
- **Decisões fundacionais:** DF2 (política sem I/O), DF4 (fail-closed), DF11 (severidade)
- **Épicos:** E20 (T11, T12)
- **Relaciona:** [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md),
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md), [ADR 0019](0019-superficie-cli-tui-v2.md),
  [ADR 0003](0003-vocabulario-v2-contencao.md)

## Contexto

O `/plan` (E20-T11) é um **estado do runtime** que injeta uma regra Enforced “escrita só sob
`.katu/`”, registada no log; os planos são `.md` densos em `.katu/plan/yymmddhhmmZ-*.md`. O
vocabulário fechado de política (`Enforcement`) só sabia negar escrita **sob** uma raiz
(`DenyWrite { root }`), e essa negação é destrancada por uma capacidade (`Workspace`/`WritePath`).
Não havia forma de exprimir “negar escrita **fora** de uma raiz” sem manipular capacidades — o que
seria implícito e frágil.

## Decisão

1. **Novo `Enforcement::DenyWriteOutside { root }`**: nega a escrita quando um caminho resolvido
   **não** está sob `root`; **nenhuma capacidade destranca** (é um muro duro). A evidência aponta o
   caminho concreto.
2. **`POLICY_VOCAB_VERSION` sobe para 3.** O vocabulário é fechado e versionado: alargá-lo é uma
   decisão de kernel registada (esta). Os `policy/*.toml` acompanham a versão.
3. **Modo plano = estado do runtime.** `Runtime` liga/desliga o modo; ao ligar, **injeta** no
   `RuleSet` a regra `plan-write-only-katu` (`Severity::Critical`, `RuleCategory::Enforced`,
   `DenyWriteOutside { root: <projecto>/.katu }`) e regista-a no log (`plan.mode`); ao desligar,
   remove-a. O estado não é persistido na sessão (retomar não reabre o modo).
4. **Artefacto de plano.** Ao entrar, escreve-se um esqueleto denso em
   `.katu/plan/<yymmddhhmmZ>-<slug>.md` (UTC), que o modelo preenche (as escritas sob `.katu/` são
   permitidas). O nome segue o formato UTC de Hinnant (sem dependência de data).
5. **`!<cmd>` (E20-T12)** executa shell **pela política/contenção** (tool `exec`, como a tool
   `bash`) e é **recusado no modo plano** (o muro `DenyWriteOutside` não cobre exec; a recusa é
   explícita antes do dispatch).

## Alternatives considered

1. **`DenyWrite { root: "/" }` crítico + retirar a capacidade de escrita do workspace.** Rejeitada:
   exigiria alterar `facts_from`/`State::workspace` no kernel e a semântica ficaria dependente de
   capacidades implícitas; um erro de composição abria o muro.
2. **Verificação ad-hoc no runtime (sem regra).** Rejeitada: não é “uma regra registada no log” e
   duplicaria a decisão de política fora do motor.
3. **Manter `POLICY_VOCAB_VERSION = 2`.** Rejeitada: a versão identifica o conjunto de `Enforcement`
   válidos; um TOML v2 com `deny_write_outside` passaria a ser aceite por um motor novo e recusado
   por um antigo — a versão deixaria de ser um contrato.
4. **Gerar o plano pelo `submit_plan` do knudge.** Adiada: o firewall só deixa `crate::memory::*`
   tocar no knudge e o artefacto `.katu/plan/*.md` é do katu, não uma nota do knudge. O esqueleto
   `.md` fica no katu; a ponte para o sistema de tarefas é trabalho futuro.

## Consequências

- A política ganha um muro duro reutilizável (jail lógica de escrita), testável pelo caminho real.
- Qualquer `RuleSet` persistido tem de declarar `vocab = 3` (fail-closed para versões antigas).
- O modo plano é por processo; um retomar não o restaura (documentado em [`CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md)).
