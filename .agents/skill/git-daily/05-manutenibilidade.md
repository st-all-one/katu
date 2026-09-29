# 05 — Manutenibilidade (Disciplina, Inspeção e Recuperação)

Fonte: `git-commit.adoc`, `git-rebase.adoc`, `git-bisect.adoc`, `git-blame.adoc`,
`git-log.adoc` + `revisions.adoc`/`gitrevisions.adoc`, `git-notes.adoc`,
`git-range-diff.adoc`, `git-reflog.adoc`, `git-reset.adoc`, `git-restore.adoc`,
`git-worktree.adoc`, `githooks.adoc`, `SubmittingPatches`, `CodingGuidelines`, `gitworkflows.adoc`.

## 1. Disciplina de commits

| Técnica | Comando | Por que |
|---|---|---|
| Commit atômico | `git add -p` + `git commit` | cada commit = mudança coerente → `bisect`/`revert` precisos |
| Mensagem 50/72, imperativo, what&why | `git commit -m "refactor: extract parse()"` | histórico legível acelera revisão |
| Emendar último | `git commit --amend` | corrige sem poluir (só em local) |
| Sign-off (DCO) | `git commit -s` | `Signed-off-by:` (ver 02-autenticidade) |
| Trailers | `git commit -m "..." -m "Co-authored-by: A <a@x>"` | crédito sem reescrever msg |
| fixup/squash | `git commit --fixup=<rev>` / `--squash=<rev>` | prepara rearranjo |
| Aplicar fixups | `git rebase --autosquash <base>` (`rebase.autoSquash=true`) | série limpa antes de publicar |
| Reorganizar | `git rebase -i <base>` (pick/reword/edit/squash/fixup/exec/drop) | histórico por tema lógico |

`rebase.abbreviateCommands=true` encurta o todo-list do `rebase -i`.

## 2. Reescrevendo histórico publicado

- **NUNCA** rebase/reset de commits já `push`-ed: quebra clones alheios (obriga merge espúrio).
  Regra: reescreva **só branch de tópico local**.
- Transplantar tópico de base:
  ```bash
  git rebase --onto master next topic        # replay de next..topic sobre master
  git rebase --onto topicA~5 topicA~3 topicA
  ```
- `rerere` (reuse recorded resolution) automatiza re-resolução em rebases repetidos:
  ```bash
  git config rerere.enabled true
  git config rerere.autoUpdate true
  git rerere forget <pathspec>   # limpar resolução ruim
  ```
- Recuperação de upstream rebasado: `git rebase --onto <upstream> <old-fork-point> <branch>`
  (fork-point evita reaplicar commits já no upstream).
- Regra de ouro integrada: **merge upwards, cherry-pick downward** (ver `01-fluxo.md`).

## 3. Estratégia de branches

```bash
git switch -c mytopic origin/master   # nascer do ponto estável correto
... hack, commit ...
git fetch origin
git rebase origin/master              # manter tópico atualizado (NÃO merge por hábito)
git switch master && git merge mytopic   # integrar (merge upward)
```
- Tópico → **rebase** sobre upstream (histórico linear, bisect limpo).
- Integração em branch compartilhada → **merge** (preserva topologia).

## 4. Bisect para depuração

| Ação | Comando | Fonte |
|---|---|---|
| Iniciar + marcar | `git bisect start` · `bad` · `good <rev>` | git-bisect |
| Restrito + pathspec | `git bisect start HEAD HEAD~10 -- <path>` | git-bisect |
| Automatizar | `git bisect run make` (exit 0=good, 1..124/126..127=bad, 125=skip) | git-bisect-lk2009 |
| Pular | `git bisect skip [<rev>|<range>]` | git-bisect |
| Visualizar | `git bisect visualize` / `git bisect log` | git-bisect |
| Termos custom | `git bisect old` / `git bisect new` | git-bisect |

> Commits atômicos e mensagens boas tornam cada ponto de parada fácil de julgar.
> `git-bisect-lk2009` recomenda "small logical commits, topic branches, no evil merges".

## 5. Blame & arqueologia

| Comando | Para que | Fonte |
|---|---|---|
| `git blame -w -M -C -L 10,20 --ignore-revs-file .git-blame-ignore-revs <file>` | `-w` whitespace, `-M`/`-C` move/cópia, esconde reformatação | git-blame |
| `git blame --since=3.weeks -- <file>` | quem mudou num período | git-blame |
| `git log -S"frotz"` / `git log -G"regex"` | pickaxe: commit que add/remove símbolo | git-log, diff-options |
| `git log --follow -- <file>` (`log.follow=true`) | segue renome | git-log |
| `git log -L 10,20:<file>` / `git log -L :func:file.c` | histórico de linha/função | git-log, line-range |
| `git log --grep= --author= --since= --until= --reverse` | navegação por msg/autor/data | git-log |
| `git log --oneline --graph --decorate` | visão de ramificação compacta | git-log |

**Revisão visual (`difftool`)** — reduz carga cognitiva em diffs grandes vs diff textual:
```bash
git config --global diff.tool meld
git difftool -d                      # abre ferramenta visual (Meld/Beyond Compare, etc.)
git difftool <revA> <revB> -- <path>
```
Combine com `git notes` (seção 6) para anotações de revisão sem alterar o hash.

## 6. Range-diff / Notes / Reflog (revisão & recuperação)

**range-diff** — compara duas séries de patches (revisão de reroll):
```bash
git range-diff <base>..<v1> <base>..<v2>
git format-patch --range-diff=v1 -3 v2   # inclui no cover-letter
```

**notes** — anexa metadados sem reescrever o commit:
```bash
git notes add -m 'Tested-by: X <x@y>' 72a144e2
git notes append -m 'Reviewed-by: ...' <commit>
```

**reflog** — rede de segurança (toda mudança de HEAD/ref é logada localmente):
```bash
git reflog                         # git log -g --abbrev-commit
git reset --hard <sha-do-reflog>   # recuperação
git reflog expire --expire=now --all
```

**reset vs restore**:
```bash
git reset --hard <commit>     # descarta working tree + index (perigoso)
git reset --keep <commit>     # mantém mudanças locais não conflitantes
git restore --staged <file>   # unstage só
git restore -s master~2 Makefile
```

## 7. Gerenciamento de contexto: stash e worktrees

**Stash** (pausa curta / troca rápida de contexto):
```bash
git stash -u                  # guarda tracked + untracked
git stash -p                  # interativo: stasha só partes de arquivos
git stash pop                 # restaura o topo
git stash list / show / drop  # inspeção/limpeza
```
`-u` inclui não-trackeados; `-p` separa mudanças acidentais de uma sessão. Use `stash`
para pausa curta; prefira **worktree** para trabalhar em outra branch por mais tempo.

**Worktrees** (paralelismo sem stash):
```bash
git worktree add ../hotfix -b emergency-fix master
git worktree add ../try-next next
git worktree list / prune / remove / lock --reason "..."
```
Casos: manter `master` limpo enquanto testa; compilar versão antiga enquanto desenvolve
outra; revisar um PR em `feature/B` enquanto coda em `feature/A` — sem perder estado e sem
ficar fazendo `stash`/`checkout` o tempo todo. CI local de branch distinto.

## 8. Hooks como quality gates

Em `.git/hooks/` (samples desabilitados por `.sample`; renomear p/ ativar).
`core.hooksPath` pode apontar hooks compartilhados.

| Hook | Dispara | Gate típico |
|---|---|---|
| `pre-commit` | antes do commit (após stage) | lint, formatação, testes rápidos |
| `prepare-commit-msg` / `commit-msg` | gera/valida msg | trailers, bloquear sujeito >50, `interpret-trailers` |
| `pre-push` | antes de `git push` | suite completa, branches proibidos |
| `pre-receive` / `update` / `post-receive` | no servidor (push) | políticas de branch, notificações, bloquear force-push em `main` |

## Regra síntese

Commits pequenos + mensagens imperativas (50/72, what&why) + trailers →
`rebase -i --autosquash` só em local → publicar via merge upward →
`bisect`/`blame`/`log -S -L` eficazes → `notes`/`range-diff` para review sem reescrever →
`reflog`/`reset --keep`/`restore` para recuperação → `worktree` para paralelismo →
`hooks` fecham o gate de qualidade.
