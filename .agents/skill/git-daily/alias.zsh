# ============================================================================
# alias.zsh — aliases de shell otimizados e alinhados a .gitconfig.example
# Requer o [alias] do .gitconfig.example ativo (st, lg, ll, a, ap, c, ca, ft,
# rb, rbm, pp, ppf, bi, bl, pick, unstage, rl). Aqui só mapeamos atalhos curtos
# para esses git-aliases, evitando duplicar lógica/flags.
# Comentários em português. Para usar: `source alias.zsh` no seu .zshrc.
# ============================================================================

# ----------------------------------------------------------------------------
# Status / inspeção
# ----------------------------------------------------------------------------
alias gs='git st'                          # status -sb
alias gst='git st'
alias gl='git lg'                          # log gráfico colorido (formato no .gitconfig)
alias gll='git ll'                         # log --oneline --graph --decorate
alias gd='git diff'
alias gds='git diff --staged'              # diff da área de stage
alias gdt='git difftool -d'                # diff visual (Meld/Beyond Compare)
alias grl='git rl'                         # reflog (recuperação)
alias gbl='git bl'                         # blame c/ ignore-revs
alias gpick='git pick'                     # log -S  → uso: gpick "simbolo" -- <path>

# ----------------------------------------------------------------------------
# Stage / commit
# ----------------------------------------------------------------------------
alias gad='git a'                          # git add
alias ga='git ap'                          # git add -p (hunks interativos)
# AVISO: 'gc' MASCARA o comando nativo 'git gc' (garbage collect). Use 'git gc'
# explícito quando precisar, ou renomeie para 'gcm'.
alias gc='git c'                           # commit -S -s -v (assina+signoff+diff)
alias gca='git ca'                         # commit --amend (mesmas flags)
alias gcm='git commit -S -s -v -m'         # commit com mensagem direta

# ----------------------------------------------------------------------------
# Sincronização (push seguro via .gitconfig)
# ----------------------------------------------------------------------------
alias gf='git ft'                          # fetch -p (com prune)
alias gpl='git pp'                         # push --follow-tags
alias gplf='git ppf'                       # push --force-with-lease (NUNCA -f)
alias gup='git pull'                       # pull rebase (pull.rebase=true no .gitconfig)
alias grb='git rb'                         # rebase -i --autosquash
alias gbm='git rbm'                        # rebase --rebase-merges

# ----------------------------------------------------------------------------
# Navegação / branches
# ----------------------------------------------------------------------------
alias gco='git checkout'
alias gsw='git switch'
alias gb='git branch'
alias groot='cd "$(git rev-parse --show-toplevel)"'   # vai à raiz do repo

# ----------------------------------------------------------------------------
# Investigação / bugs / cirurgia
# ----------------------------------------------------------------------------
alias gbi='git bi'                         # bisect start
alias gcp='git cherry-pick'
alias grv='git revert'                     # desfaz em branch PÚBLICA
alias gunstage='git unstage'               # restore --staged
alias gwt='git worktree'                   # worktree add/list/remove
alias gsh='git stash'                      # stash
alias gshp='git stash -p'                  # stash interativo
alias gshpop='git stash pop'

# ----------------------------------------------------------------------------
# Recovery / manutenção local
# ----------------------------------------------------------------------------
alias gfsck='git fsck --unreachable'       # encontra commits/objetos pendentes
alias gclean='git clean -fd'               # remove untracked (CUIDADO)
