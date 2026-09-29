# 01 — Fluxo de Trabalho (Workflow)

Fonte: `gitworkflows.adoc`, `giteveryday.adoc`, `git-rebase.adoc`, `git-merge.adoc`.

## Regra central: separe mudanças em passos lógicos

> Split your changes into small logical steps, and commit each of them.

Cada commit deve ser consistente, compilar/passar nos testes e funcionar de forma
independente. Isso torna a revisão mais fácil e o histórico útil para inspeção futura
(`git blame`, `git bisect`). É mais fácil *squashar* commits pequenos do que dividir
um commit gigante. Use `git rebase --interactive` para reorganizar antes de publicar.

```bash
git add -p                 # stage por hunk → commits atômicos
git commit -m "msg"       # avança o branch
git rebase -i <base>      # reorganiza (squash/reorder/fixup) ANTES do push
```

## Branches de tópico (topic branches)

> Make a side branch for every topic (feature, bugfix, ...). Fork it off at the
> oldest integration branch that you will eventually want to merge it into.

- Crie um branch por feature/bugfix: `git switch -c topic/feature`.
- Nomeie por prefixo (`topic/`, `fix/`) para facilitar filtros e `git show-branch`.
- **Não faça merge de downstream para upstream por hábito** — só com boa razão
  (mudança de API upstream, conflito de merge). Merge constante polui o histórico.

## Merge upwards, cherry-pick downward

Integração em projetos com branches de estabilidade (`maint` → `master` → `next`/`seen`):

> Always commit your fixes to the oldest supported branch that requires them.
> Then merge the integration branches upwards into each other.

- Correções entram na branch **mais antiga** que as precisa.
- **Merge** sobe a integração (`topic` → `next` → `master` → `maint`).
- **Cherry-pick** desce correções pontuais (`git cherry-pick <rev>`).
- Merge opera no nível de branch (escala para 1..1000 commits); cherry-pick no nível de commit.

## Throw-away integration

Para testar a interação de vários tópicos sem sujar o histórico estável:

> To test the interaction of several topics, merge them into a throw-away branch.
> You must never base any work on such a branch!

```bash
git switch -C seen next
git merge topic/one topic/two
# testa; depois descarta (branch efêmero)
```

## Publicação e integração (workflows distribuídos)

Dois modelos paralelos (gitworkflows.adoc):

**Merge workflow** (histórico completo, incluindo merges):
```bash
git fetch <remote>                    # manter-se atualizado
git push  <remote> <branch>           # publicar tópico
git pull  <url> <branch>              # mantenedor: fetch+merge de tópico remoto
# NÃO use `git pull` a menos que queira de fato mesclar o branch remoto.
```

**Patch workflow** (e-mails, sem merges):
```bash
git format-patch -M upstream..topic   # gera patches
git send-email --to=<dest> 00*.patch  # envia sem corromper por MUA
git am < patch                        # mantenedor importa
git am -3 < patch                     # three-way merge se houver conflito
```

Mantenedor típico (`giteveryday.adoc`):
```bash
git branch --no-merged master         # o que falta integrar
git am -3 -i -s ./+to-apply           # aplica com sign-off
git rebase master                     # rebase de tópico ainda não publicado
git tag -s -m "Release X.Y.Z" vX.Y.Z  # tag assinada
git push --follow-tags ko             # publica histórico + tags
```

## Lançamento (release)

- Feature release vem de `master`; maintenance release vem de `maint`.
- Garanta que `master` é superset de `maint`:
  ```bash
  git log master..maint     # deve estar vazio
  ```
- Tag assinada no tip e push:
  ```bash
  git tag -s -m "Git X.Y.Z" vX.Y.Z master
  git push --follow-tags <remote>
  ```

## Anti-padrões

- Commits "everything works" monolíticos → `bisect` inútil.
- Merge de `master` para `topic` por hábito → histórico cluttered.
- `git pull` automático sem querer mesclar → prefira `git fetch` + revisão.
- Rebase de branch já compartilhado → quebra clones alheios.
