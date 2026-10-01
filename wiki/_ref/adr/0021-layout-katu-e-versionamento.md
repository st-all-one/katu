# ADR 0021 — Layout central do `.katu/` e versionamento

- **Estado:** aceite
- **Data:** 2026-10-09
- **Decisões fundacionais:** DF4 (fail-closed), DF7 (superfície contida), G4 (memória de primeira
  classe)
- **Épicos:** E20 (T18, T19), E03, E06
- **Relaciona:** [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md),
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md),
  [ADR 0020](0020-configuracao-global-local.md)

## Contexto

O `.katu/` já existia, mas só com `sessions/` e `audit/` (criados por `Session::create`, que ainda
garantia a exclusão da auditoria em `.git/info/exclude`). Faltava um **layout central** para a
memória (`knowledge/`), travas (`guardrails/`), lixo (`trash/`), log (`log/`) e planos (`plan/`), e
uma política de versionamento previsível: o que é do utilizador (config, notas, travas) deve poder
ser versionado; o que é efémero ou sensível (auditoria, lixo, log) nunca.

## Decisão

1. **Layout** (idempotente): `.katu/{katu.toml, sessions, audit, knowledge, knowledge/.idx,
   guardrails, trash, log, plan}`.
2. **Sempre fora do git:** `.katu/audit/`, `.katu/trash/`, `.katu/log/` (bloco gerido no
   `.gitignore`).
3. **O resto segue `git.versioned`** (config, default `true`). Quando `false`, `.katu/` é excluído
   no `.git/info/exclude` (bloco gerido); quando `true`, o bloco é removido.
4. **`katu --init [--git-excluded|--git-tracked]`** força o modo; sem flag, segue `git.versioned`.
   Os ficheiros de git (`.gitignore`, `.gitattributes`, `.git/info/exclude`) são **criados se
   faltarem** e só quando o projeto está num repositório (`.git` presente).
5. **Snapshot da config:** o `init` (e o arranque de sessão) copia a config global **1:1** para
   `<projeto>/.katu/katu.toml`; sem global, escreve o default (ADR 0020).
6. **Travas padrão:** `guardrails/` copia as travas globais (`<config>/katu/guardrails/`) para o
   projeto, só as que faltarem; o projeto é a fonte local.
7. **Blocos geridos:** todos os ficheiros de git usam marcadores `# katu — gerido (início/fim)`,
   substituíveis sem tocar no conteúdo manual.
8. **`--init --force`** refaz o bootstrap preservando **só** o conhecimento personalizado
   (`knowledge/`, `guardrails/`, `audit/`): config, `trash/`, `log/`, `plan/` e `sessions/` são
   removidos e recriados. É a operação de reset explícito — nunca implícita.

## Alternatives considered

1. **Versionar o `.katu/` inteiro (incl. audit/trash/log).** Rejeitada: a auditoria e o log são
   efémeros/volumosos e o lixo é recuperável; versioná-los polui o histórico.
2. **Nunca versionar o `.katu/`.** Rejeitada: a config, as notas e as travas são conhecimento do
   projeto e devem poder viajar com ele.
3. **Guardar o layout só em memória (sem dirs).** Rejeitada: a memória e as travas precisam de um
   lar no disco, e o `init`/`doctor --fix` têm de ser verificáveis.
4. **Excluir via `.gitignore` em vez de `.git/info/exclude`.** Rejeitada para o caso "árvore
   inteira": `--git-excluded` é uma escolha **local** (não deve ser commitada); as exclusões
   permanentes (audit/trash/log) ficam no `.gitignore` gerido.
5. **Copiar as travas a cada arranque.** Rejeitada: sobrescreveria edições locais; copia-se só o que
   falta.
6. **`--force` apagar tudo (incl. conhecimento).** Rejeitada: o conhecimento personalizado
   (notas/travas/auditoria) é caro de reconstruir; o reset preserva-o e refaz só o resto.

## Consequências

- **Positivas:** um lar único e previsível; versionamento explícito e reversível; `--init` e
  `memo doctor --fix` idempotentes; nenhuma exclusão depende de estado escondido.
- **Negativas / dívida:** os blocos geridos partilham o ficheiro com conteúdo manual (resolvido por
  marcadores); o worker de embeddings (`memo drain --watch-service`) ainda não existe (E20-T20).
- **Travas:** testes de idempotência do `--init`, de `--git-excluded`, de `--init --force`
  (preserva conhecimento, reseta o resto) e de `doctor --fix`; `check-surface`; o uso vive em
  [`docs/CLI_TUI_SURFACE.md`](../docs/CLI_TUI_SURFACE.md).
