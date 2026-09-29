# 06 — Cheat Sheet (Referência Rápida)

Síntese operacional do dossiê. Comandos/flags em inglês; prosa em português.

## Sintaxe de revisões (`gitrevisions.adoc`, `revisions.adoc`)

| Sintaxe | Significado |
|---|---|
| `HEAD~n` | n-ésimo ancestral em linha reta (pai¹ repetido) |
| `HEAD^` / `HEAD^2` | primeiro / segundo pai de um merge |
| `A..B` | alcançáveis de B mas não de A (`B --not A`) |
| `A...B` | alcançáveis de A ou B, mas não de ambos (simétrico) |
| `@{u}` / `@{upstream}` | branch de rastreamento upstream do atual |
| `@{1 week ago}` / `master@{3}` | reflog: estado há 1 semana / 3 movimentos atrás |
| `:/text` (ou `:/fix crash`) | commit mais recente cuja mensagem casa com `text` |
| `<rev>:<path>` | objeto em path na revisão (`HEAD:Makefile`, `v1.0:src/x.c`) |

## Setup essencial (segurança + autenticidade)

```bash
git config --global user.name  "Nome"
git config --global user.email "voce@exemplo.com"
git config --global gpg.format ssh
git config --global user.signingKey "~/.ssh/id_ed25519.pub"
git config --global gpg.ssh.allowedSignersFile "~/.git_allowed_signers"
git config --global commit.gpgSign true
git config --global tag.gpgSign true
git config --global credential.helper libsecret
git config --global transfer.credentialsInUrl die
git config --global transfer.fsckObjects true
git config --global safe.bareRepository explicit
git config --global init.defaultBranch main
```

## Fluxo diário (manutenibilidade)

```bash
git switch -c topic/feature origin/main     # branch de tópico
git add -p && git commit -m "feat: ..."     # atômico + mensagem 50/72
git commit --fixup=<rev>                     # prepara conserto
git fetch origin && git rebase origin/main   # atualiza (não merge por hábito)
git rebase -i --autosquash origin/main       # consolida antes do push
git switch main && git merge --no-ff topic/feature   # merge upward
git push --signed --follow-tags origin
```

## Inspeção e depuração

```bash
git log --oneline --graph --decorate --all
git log -S"simbolo" -- <path>                # pickaxe
git log -L :func:file.c                       # histórico de função
git blame -w -M -C --ignore-revs-file .git-blame-ignore-revs <file>
git bisect start; git bisect bad; git bisect good v1.0
git bisect run make                           # automatizado
git reflog                                    # recuperação
git notes add -m 'Reviewed-by: X' <commit>    # sem reescrever
git range-diff v1~3..v1 v2~3..v2             # revisão de reroll
git difftool -d                               # revisão visual (Meld/Beyond Compare)
```

## Troca de contexto

```bash
git stash -u                       # guarda tracked + untracked
git stash -p                       # stash interativo (partes de arquivo)
git stash pop                     # restaura
git worktree add ../hotfix -b fix master   # branch paralela em outra pasta
git cherry-pick <sha>             # traz commit específico (hotfix)
git revert <sha>                  # desfaz em branch PÚBLICA (não use reset)
```

## Performance (rotina)

```bash
git maintenance start                          # agendado, sem travar
git commit-graph write --reachable --changed-paths
git multi-pack-index write --bitmap
git count-objects -vH                          # diagnóstico
git gc --auto
# status/diff mais rápidos:
git config --global core.untrackedCache true
git config --global core.fsmonitor true
git config --global diff.relative true
```

## Repo gigante / clone enxuto

```bash
git clone --filter=blob:none --sparse <url>
git sparse-checkout set src/docs
git clone --depth 1 <url>                      # só CI/release, não dev pleno
```

## Backup offline (air-gapped)

```bash
git bundle create backup.bundle --all
git bundle verify backup.bundle
git clone backup.bundle workdir
```

## Recuperação de desastres

```bash
git reflog                                     # acha o SHA anterior
git reset --keep <sha>                         # mantém mudanças locais
git restore --staged --worktree <file>         # descarta tudo de um arquivo
git rebase --onto <upstream> <old-fork-point> <branch>   # upstream rebasado
```

## Anti-padrões (não faça)

- `git commit -am "wip"` monolítico → `bisect` inútil.
- `git pull` automático sem querer mesclar → prefira `fetch` + revisão.
- rebase de branch já compartilhado → quebra clones alheios.
- `credential.helper store` em máquina compartilhada → vaza senha em disco.
- `git filter-branch` para limpar segredos → use `git filter-repo`.
- `git://` em rede aberta → use `ssh://`/`https://`.
- shallow clone para desenvolvimento pleno → prejudica `bisect`/`blame`.
