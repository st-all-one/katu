---
name: git-daily
description: Dense, token-optimized reference for correct day-to-day Git usage focused on investigation and bug debugging — bisect, blame, log pickaxe, reflog recovery, range-diff, rerere, surgical rewriting, and safe recovery. Load when tracing regressions, finding the commit that introduced a bug, inspecting authorship, recovering lost commits, comparing patch series, or performing surgical history edits. Not for global config/alias setup.
category: tooling
version: "1.0"
tags: [git, bisect, blame, reflog, debugging, history-rewrite, recovery, version-control]
license: MIT
---

# git-daily — Investigation & Bug Debugging

## Triggers
Load for: regression tracing, "which commit broke X", authorship/blame, lost-commit
recovery, patch-series comparison, or surgical history edits. Not for aliases/global config.

## Non-negotiable rules
- Atomic commits; **never rewrite published/shared history**.
- Sign: `commit -S`, `push --signed=if-asked`.
- Force → always `--force-with-lease`, own branch only.
- Never commit secrets; if done: rotate + `git filter-repo`/BFG, not just delete/force.
- Recover via reflog **before** `gc`/expire.

## Investigation toolkit

### git bisect — locate the breaking commit
```bash
git bisect start
git bisect bad                       # current HEAD is broken
git bisect good v1.0                 # last known-good
git bisect good | bad                # test + mark each step
git bisect run ./test.sh             # automated: exit 0=good, 125=skip, else=bad
git bisect skip                     # untestable commit
git bisect visualize / log          # inspect
git bisect reset                    # return to original HEAD
```

### git blame — who/what, noise-ignoring
```bash
git blame -w -M -C -C <file>                       # ignore ws; detect moved/copied lines
git blame --ignore-revs-file .git-blame-ignore-revs <file>  # skip refactors
git blame -L 10,20 <file>
```
`.git-blame-ignore-revs` = list of commits (e.g. bulk reformats) to exclude.

### git log — pickaxe, function, history
```bash
git log -S"symbol" -- <path>        # pickaxe: when string appeared/disappeared
git log -G"regex" -- <path>         # when diff matches regex
git log -L :func_name:file.c        # history of a function/line
git log --follow -- <file>          # follow renames
git log --oneline --graph --decorate --all --first-parent   # clean main view
git log -p --author= --since= --grep= --invert-grep --grep=WIP
git log --show-signature            # verify authorship
git log --reflog                    # include reflog entries
```

### git reflog — local recovery net
```bash
git reflog                          # or: git log -g
git reset --hard <sha-from-reflog>  # undo bad reset/rebase/merge
git reflog expire --expire=now --all   # ONLY after confirming unneeded
```
Local, expires (~90d); act before `gc`.

### range-diff / notes — series & annotation
```bash
git range-diff v1~3..v1 v2~3..v2    # diff between patch rerolls
git notes add -m 'Reviewed-by: X' <sha>   # annotate without rewriting hash
git notes show <sha>
```

### difftool — visual diff for large changes
```bash
git difftool -d                     # directory diff (needs one-time diff.tool set)
git difftool <a> <b> -- <path>
```

## Surgical operations

### cherry-pick / revert vs reset
```bash
git cherry-pick <sha>               # bring one commit (hotfix)
git revert <sha>                    # undo on PUBLIC branch → new commit
git reset --hard <sha>              # undo on LOCAL/private branch only
git reset --keep <sha>              # keep non-conflicting local changes
git restore --staged <file>         # unstage only
git restore -s <rev> -- <file>      # restore file from rev
```
Rule: `revert` for shared history; `reset` for private only.

### rebase — clean history before sharing
```bash
git rebase -i --autosquash origin/main     # reorder/fixup
git rebase --onto <newbase> <oldbase> <branch>   # transplant
git rebase -i --rebase-merges               # preserve merge topology
git rebase -x "make test"                   # run cmd after each commit
git rebase -S                               # re-sign during rebase
```
Never rebase published branches.

### rerere — reuse conflict resolution (one-time enable)
```bash
git rerere status | diff | forget <path>   # (after rerere.enabled=true)
```
Identical future conflicts auto-resolve once enabled.

### stash / worktree — context switching
```bash
git stash -u                      # tracked + untracked
git stash -p                      # interactive (partial)
git stash pop
git worktree add ../hotfix -b fix origin/main   # parallel branch, separate dir
git worktree list | remove
```

## Safety during investigation
- `--force-with-lease` > `-f`.
- `git fsck --unreachable` → find dangling commits/objects.
- `safe.directory` if repo owned by another user (avoid blind `-c safe.directory=*`).
- Scope diffs: `git show --stat <sha>`, `git diff <a> <b> -- <path>`.

## Quick reference
| Need | Command |
|---|---|
| Find breaking commit | `git bisect run <script>` |
| Who changed this line | `git blame -w -M -C -C --ignore-revs-file .git-blame-ignore-revs <f>` |
| When symbol changed | `git log -S"sym" -- <path>` |
| Function history | `git log -L :fn:file` |
| Recover lost commit | `git reflog` → `git reset --hard <sha>` |
| Compare rerolls | `git range-diff A B` |
| Undo on public | `git revert <sha>` |
| Undo on private | `git reset --hard <sha>` |
| Parallel work | `git worktree add <dir> -b <b> <base>` |
