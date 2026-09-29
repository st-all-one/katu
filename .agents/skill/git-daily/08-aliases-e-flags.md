# 08 — Flags Essenciais e Aliases (Comandos do Dia a Dia)

As flags mais úteis de `add`, `commit`, `pull`, `push`, `fetch`, `rebase`, `log` (e
`status`) consolidadas numa referência única, com uso recomendado e o bloco de aliases.
Fontes: `git-add.adoc`, `git-commit.adoc`, `git-pull.adoc`, `git-push.adoc`,
`git-fetch.adoc`, `git-rebase.adoc`, `git-log.adoc`, `git-status.adoc`, `git-config.adoc`.

Filosofia de alias: torne o comando **mais seguro e mais informativo**, nunca o contrário.
Um alias que esconde comportamento destrutivo (`gpf='git push --force'`) é uma arma — use
`--force-with-lease`. E evite mascarar comandos nativos (ver gotchas abaixo).

---

## git status

| Flag | Efeito |
|---|---|
| `-s` / `--short` | formato compacto (uma linha por arquivo) |
| `-b` / `--branch` | mostra branch atual + upstream (o combo `-sb`) |
| `-uno` / `--untracked-files=no` | não lista untracked (mais rápido) |
| `-uall` | lista untracked recursivamente |
| `-v` | mostra também o diff da área de stage |

Recomendado: `gs='git status -sb'`. (Combine com `core.untrackedCache`/`core.fsmonitor`
do pilar Performance para velocidade.)

---

## git add

| Flag | Efeito | Quando |
|---|---|---|
| `-p` / `--patch` | stage por hunk interativo | **padrão para commits atômicos** |
| `-N` / `--intent-to-add` | marca intenção sem conteúdo | ver diff de arquivo novo antes de commitar |
| `-i` / `--interactive` | modo interativo completo | seleção fina |
| `-A` / `--all` | tracked + untracked em toda a árvore | **cuidado**: pode stagear lixo |
| `.` | só do diretório atual | evita surpresas fora do pwd |
| `-u` / `--update` | só arquivos já trackeados | |

**Recomendado**: preferir `git add -p` a `git add -A`. `ga='git add -p'` (ou `gad='git add'`
para o caso óbvio).

---

## git commit

| Flag | Efeito | Pilar |
|---|---|---|
| `-S` / `--gpg-sign` | assina o commit | Autenticidade |
| `-s` / `--signoff` | adiciona `Signed-off-by:` (DCO) | Autenticidade |
| `-v` / `--verbose` | abre o editor **com o diff** para revisar o que vai commitar | Manutenibilidade |
| `-m` / múltiplos `-m` | sujeito + corpo | — |
| `--amend` | emenda o último commit (só local) | Manutenibilidade |
| `--fixup=<rev>` / `--squash=<rev>` | prepara p/ `rebase --autosquash` | Manutenibilidade |
| `-C <rev>` / `-c <rev>` | reusa mensagem de outro commit | — |
| `-a` / `--all` | stageia tracked modificados | **cuidado**: pode incluir tudo |
| `--no-verify` / `-n` | bypassa `pre-commit`/`commit-msg` | **DANGER**: só em emergência real |
| `--reset-author` / `--date=` | corrige autor/data | — |

**Recomendado**: `gc='git commit -S -s -v'` (seu exemplo). Para commit com mensagem
direta: `gcm='git commit -S -s -v -m'`. Com `commit.gpgSign=true` o `-S` é redundante mas
explícito.

---

## git pull

`git pull` = `fetch` + `merge`. O merge implícito é a maior fonte de histórico sujo.

| Flag / Config | Efeito |
|---|---|
| `git pull --rebase` | fetch + **rebase** (história linear) |
| `pull.rebase=true` | torna o `--rebase` o padrão (recomendado) |
| `git pull --ff-only` | só avança se for fast-forward; recusa caso contrário |
| `git pull --no-commit` | faz o merge mas não commite (inspecione antes) |

**Recomendado**: configure `pull.rebase=true` e `pull.ff=only`; ou, melhor ainda,
`git fetch` + revisar + `git rebase`/`git merge` explícito (nunca `git pull` às cegas).

---

## git push

| Flag / Config | Efeito | Pilar |
|---|---|---|
| `--follow-tags` | envia tags anotadas no intervalo | Autenticidade |
| `--signed[=if-asked]` | push certificado | Autenticidade |
| `--force-with-lease` | força **só se** o remote não mudou | Segurança |
| `--force` / `-f` | força cego | **evitar** (quebra clones alheios) |
| `--dry-run` | simula sem enviar | — |
| `-u` / `--set-upstream` | marca tracking | — |
| `push.default=simple` | push só do branch atual com nome igual (default) | — |
| `receive.denyNonFastForwards` | servidor bloqueia force em branch protegida | Segurança |

**Recomendado**: `gp='git push --follow-tags'` e use `--force-with-lease` (nunca `-f`) em
rebase de branch própria. Signed push: `git push --signed=if-asked`.

---

## git fetch

| Flag / Config | Efeito |
|---|---|
| `git fetch --prune` / `-p` | remove remote-tracking branches obsoletas |
| `fetch.prune=true` | torna `--prune` o padrão (recomendado) |
| `git fetch --all` | de todos os remotes |
| `git fetch origin <branch>` | só uma branch |
| `git fetch --depth=<n>` | shallow fetch |
| `git remote update` | equivalente a fetch de todos |

**Recomendado**: `fetch.prune=true` para não acumular refs fantasma.

---

## git rebase

| Flag / Config | Efeito | Pilar |
|---|---|---|
| `-i` / `--interactive` | reorganiza commits | Manutenibilidade |
| `--autosquash` (+ `rebase.autoSquash`) | aplica fixups automaticamente | Manutenibilidade |
| `--onto <up> <fork> <branch>` | transplante cirúrgico | Manutenibilidade |
| `-x <cmd>` / `--exec` | roda comando após cada commit | Manutenibilidade |
| `--rebase-merges` / `-r` | preserva topologia de merges (vs achatar) | Manutenibilidade |
| `-S` / `--gpg-sign` | re-assina durante o rebase | Autenticidade |
| `rebase.abbreviateCommands=true` | todo-list enxuto | — |
| `rerere.enabled=true` | reusa resolução de conflito | Manutenibilidade |

**Recomendado**: `grb='git rebase -i --autosquash origin/main'`. Nunca rebaseie branch
já publicada/compartilhada.

---

## git log

| Flag | Efeito |
|---|---|
| `--oneline --graph --decorate --all` | visão de ramificação compacta |
| `--format=<fmt>` / `--pretty` | formatação custom (ver alias `gl` abaixo) |
| `-p` | mostra o patch de cada commit |
| `--stat` | resumo de arquivos alterados |
| `-S"x"` / `-G"re"` | pickaxe (quando `x` apareceu/sumiu) |
| `-L :func:file` | histórico de uma função/linha |
| `--follow` | segue renome de arquivo |
| `--author=` / `--committer=` / `--grep=` | filtros |
| `--since=` / `--until=` | por data |
| `--first-parent` | segue só o primeiro pai (visão limpa de `main`) |
| `--no-merges` / `--merges` | filtra merges |
| `--show-signature` | mostra/verifica assinatura |
| `--reflog` | inclui entradas do reflog |
| `--invert-grep --grep=WIP` | exclui commits com WIP |

**Recomendado** (seu exemplo, canônico):
```bash
alias gl='git log --graph --abbrev-commit --decorate --format=format:"%C(bold blue)%h%C(reset) - %C(bold green)(%ar)%C(reset) %C(white)%s%C(reset) %C(dim white)- %an%C(reset)%C(bold yellow)%d%C(reset)" --all'
```

---

## Bloco de aliases recomendado (`~/.gitconfig`)

```ini
[alias]
    # status / inspeção
    st   = status -sb
    lg   = log --graph --abbrev-commit --decorate --format=format:"%C(bold blue)%h%C(reset) - %C(bold green)(%ar)%C(reset) %C(white)%s%C(reset) %C(dim white)- %an%C(reset)%C(bold yellow)%d%C(reset)" --all
    ll   = log --oneline --graph --decorate
    # stage / commit
    a    = add
    ap   = add -p
    c    = commit -S -s -v
    ca   = commit -S -s -v --amend
    # branches / sincronização
    ft   = fetch -p
    rb   = rebase -i --autosquash
    rbm  = rebase --rebase-merges
    # push seguro
    pp   = push --follow-tags
    ppf  = push --force-with-lease
[commit]
    gpgsign = true
[pull]
    rebase = true
    ff = only
[fetch]
    prune = true
[rebase]
    autoSquash = true
    abbreviateCommands = true
```

---

## Gotchas de alias

- **Não mascare comandos nativos.** `gc='git commit ...'` (convenção oh-my-zsh) **esconde**
  `git gc` (garbage collect). Se precisar de `git gc`, terá de digitar o caminho completo.
  Prefira `gc` para commit e use `git gc` explícito, ou nomeie o alias de commit como `cm`.
- **`--force` nunca como atalho mudo.** Se criar `gpf`, faça apontar para
  `--force-with-lease`, não `-f`.
- **`-a` em commit é armadilha.** `git commit -a` stageia *todos* os tracked modificados;
  prefira `add -p` para controlo.
- **`--no-verify` é emergência, não rotina** — contorna `pre-commit` (scan de segredos).
- **Alias de `pull` com `--rebase`** é seguro e recomendado; alias de `pull` sem nada é o
  que gera merges espúrios.
