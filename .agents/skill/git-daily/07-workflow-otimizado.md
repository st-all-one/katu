# 07 — Workflow Padrão Otimizado

Resumo executivo e operacional de um fluxo diário que integra os quatro pilares
(autenticidade, segurança, performance, manutenibilidade) numa rotina coerente.
Baseado em `gitworkflows.adoc`, `giteveryday.adoc`, `git-commit`, `git-rebase`,
`git-maintenance`, `gitformat-signature`, `gitcredentials`, `githooks`.

## 0. Fundação (uma vez por máquina)

```bash
# Identidade
git config --global user.name  "Seu Nome"
git config --global user.email "voce@exemplo.com"
git config --global init.defaultBranch main

# Autenticidade (assinatura SSH, sem GPG)
git config --global gpg.format ssh
git config --global user.signingKey "~/.ssh/id_ed25519.pub"
git config --global gpg.ssh.allowedSignersFile "~/.git_allowed_signers"
git config --global commit.gpgSign true
git config --global tag.gpgSign true

# Segurança de credenciais e objetos
git config --global credential.helper libsecret      # nunca 'store'
git config --global transfer.credentialsInUrl die
git config --global transfer.fsckObjects true
git config --global safe.bareRepository explicit
git config --global core.hooksPath ~/.config/git/hooks   # hooks compartilhados

# Performance
git config --global core.commitGraph true
git config --global core.untrackedCache true
git config --global core.fsmonitor true
git config --global diff.relative true
git config --global rebase.autoSquash true
git config --global rebase.abbreviateCommands true
git config --global protocol.version 2
git config --global fetch.negotiationAlgorithm skipping

# Manutenção agendada (sem travar o dia a dia)
git maintenance start        # estratégia incremental
```
`.git_allowed_signers` deve conter, por pessoa, `Nome <email> ssh-ed25519 AAAA...`
(formato ALLOWED SIGNERS).

## 1. Início de tarefa — branch de tópico

```bash
git fetch origin                 # atualiza tracking sem mesclar
git switch -c topic/login origin/main
```
Regra: nascer do ponto estável mais antigo necessário; nunca fazer merge de
`origin/main` por hábito. Para pausa curta: `git stash -u` / `git stash -p`;
para revisar outra branch em paralelo: `git worktree add ../pr-b -b pr-b origin/main`.

## 2. Desenvolvimento — commits atômicos e assinados

```bash
# trabalho por hunk → commit coerente
git add -p
git commit -m "feat(login): validar token no middleware"

# conserto pontual de um commit anterior
git commit --fixup=<sha>
```
- Mensagem: sujeito ≤50, imperativo, "what & why" (não "how").
- Trailers quando aplicável: `Co-authored-by:`, `Reviewed-by:`.
- Assinatura automática via `commit.gpgSign=true`.

## 3. Manter tópico atualizado (rebase, não merge)

```bash
git fetch origin
git rebase origin/main           # história linear → bisect limpo
```
Em conflito recorrente em long-lived topic: `git config rerere.enabled true`.

## 4. Consolidar antes de publicar (só local)

```bash
git rebase -i --autosquash origin/main
# squash/fixup/reword para série final legível
git log --show-signature         # confirmar assinaturas
```

## 5. Publicar e integrar

```bash
git switch main
git merge --no-ff topic/login    # merge upward preserva topologia
git tag -s -m "v1.4.0" v1.4.0    # release assinado
git push --signed --follow-tags origin
```
Mantenedor remoto (fork): `git push --signed` + PR; never force-push em `main`.

## 6. Revisão sem reescrever

```bash
git notes add -m 'Reviewed-by: X' <sha>     # metadados persistentes
git range-diff v1~3..v1 v2~3..v2            # diff entre rerolls
git difftool -d                              # revisão visual de diffs grandes
```
Hotfix em `main`: `git cherry-pick <sha>` (traz só o commit); se precisar desfazer em
branch pública, `git revert <sha>` — **nunca** `git reset --hard` em `main`.

## 7. Depuração quando quebrar

```bash
git bisect start; git bisect bad; git bisect good v1.3.0
git bisect run make                         # isola o culprit em O(log n)
```
Só funciona bem porque os commits são pequenos e atômicos (passo 2).

## 8. Manutenção contínua (background)

```bash
git commit-graph write --reachable --changed-paths   # acelera log/merge
git multi-pack-index write --bitmap                   # acelera clone/fetch
git count-objects -vH                                 # checar ineficiência
# o git maintenance já roda gc/incremental-repack agendado
```
Repositório gigante: `git clone --filter=blob:none --sparse <url>` + `git sparse-checkout set <dirs>`.

## 9. Recuperação

```bash
git reflog                      # encontrar estado pré-erro
git reset --keep <sha>          # descartar sem perder trabalho local
git restore --staged --worktree <arquivo>
```

## 10. Quality gates (hooks)

- `pre-commit`: lint + testes rápidos + scan de segredos (gitleaks/trufflehog).
- `commit-msg`: validar sujeito ≤50 e trailers obrigatórios.
- `pre-push`: suite completa.
- Servidor (`pre-receive`/`update`): bloquear force-push em `main`, validar
  `GIT_PUSH_CERT_STATUS` se usar `git push --signed`.

## Anti-padrões deste workflow

- Commits `wip` monolíticos (inutilizam bisect).
- `git pull` automático (prefira `fetch` + `rebase`).
- Rebase de branch já compartilhado.
- `credential.helper store` em máquina compartilhada.
- `git filter-branch` (use `git filter-repo` para limpar segredos).
- `git://` em rede aberta.
- Shallow clone para dev pleno.

## Métricas de sucesso

| Sintoma | Causa provável | Ação |
|---|---|---|
| `bisect` impreciso | commits grandes | reduzir tamanho no próximo ciclo |
| push lento | repo sem bitmap/commit-graph | passos 0 e 8 |
| `status` demora | sem fsmonitor/untrackedCache | passo 0 |
| dúvida "quem mudou X" | sem assinatura/trailers | passos 0 e 2 |
| segredo no histórico | `.gitignore` falhou | `git filter-repo` + force re-clone |
