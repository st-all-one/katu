# 04 — Performance (Higiene, Packing e Velocidade)

Fonte: `git-gc.adoc` + `config/gc.adoc`, `git-maintenance.adoc` + `config/maintenance.adoc`,
`git-commit-graph.adoc` + `config/commitgraph.adoc`, `git-repack.adoc` + `config/repack.adoc`,
`git-multi-pack-index.adoc`, `git-clone.adoc`, `git-sparse-checkout.adoc`, `git-bundle.adoc`,
`git-count-objects.adoc`, `git-fsck.adoc`, `git-status.adoc`, `config/{core,pack,diff,fetch,protocol}.adoc`.

## 1. Higiene e Garbage Collection

`git gc` — housekeeping (comprime revisões, remove unreachable, packa refs, pruna reflog,
atualiza commit-graph).
- `--aggressive`: deltas mais ótimos, porém **muito mais lento**; ganhos nem sempre compensam.
- `--prune=now`: pruna loose objects sem grace period (2 weeks default). Risco de corrupção
  se houver escrita concorrente. `gc.pruneExpire` (default `2.weeks.ago`).
- `--auto`: só roda se passar de `gc.auto` (loose objects, 6700) / `gc.autoPackLimit` (packs, 50).
- `--keep-largest-pack` / `gc.bigPackThreshold`: preserva maiores packs (evita repack all-in-one).

Configs (`config/gc.adoc`): `gc.aggressiveDepth` (50), `gc.aggressiveWindow` (250),
`gc.writeCommitGraph` (true), `gc.cruftPacks` (true), `gc.reflogExpire` (90d),
`gc.reflogExpireUnreachable` (30d), `gc.packRefs` (true).

## 2. git maintenance (orquestra em background)

Não trava comandos do usuário: agenda via crontab / systemd-timer / launchctl / schtasks.

```bash
git maintenance register     # adiciona a maintenance.repo (estratégia incremental)
git maintenance start        # agenda tarefas
git maintenance run --task=<task>
```
Tarefas: `commit-graph`, `prefetch`, `loose-objects`, `incremental-repack`,
`pack-refs`, `reflog-expire`, `gc`, `geometric-repack`.

Estratégias (`maintenance.strategy`):
- `incremental` (default): prefetch+commit-graph hourly; loose-objects+incremental-repack daily;
  pack-refs weekly. **Não deleta dados**.
- `geometric`: repack geométrico — recomendado para repositórios grandes.

`incremental-repack` usa o multi-pack-index (`expire` + `repack`) sem travar concorrência.

## 3. Commit-graph

`git commit-graph write --reachable` serializa o grafo de commits (gerações / corrected
commit dates). Acelera `log`, `merge`, reachability e `git log -- <path>`.

- `--changed-paths`: Bloom filters de paths alterados → grande ganho em `git log -- <path>`
  (pode ser lento em repos grandes).
- `--split[=<strategy>]`: cadeia incremental de `.graph` (merge por `--size-multiple`).
- `core.commitGraph` (default true) faz o Git ler o grafo.
- `commitGraph.generationVersion` (default 2).

Mantido automaticamente por `gc.writeCommitGraph=true` e pela task `commit-graph`.

## 4. Packing & Multi-Pack-Index (MIDX)

`git multi-pack-index write --bitmap` indexa múltiplos packs num único MIDX (opcionalmente
com multi-pack bitmap). `core.multiPackIndex` (default true).

```bash
git multi-pack-index write --bitmap
git multi-pack-index expire      # apaga packs sem objetos referenciados
git multi-pack-index repack      # consolida packs pequenos
```

Bitmap indexes aceleram a fase "counting objects" de clone/fetch (server side):
- `repack.writeBitmaps` (true em bare repos) → bitmap no repack all-into-one.
- `pack.useBitmaps`, `pack.writeBitmapHashCache`, `pack.writeBitmapLookupTable`.

`git repack`:
- `-d` remove packs redundantes; `-A` unreachable viram loose; `-l` local (sem alternates).
- `--geometric=<factor>`: progressão geométrica de tamanhos (repack incremental, grandes repos).
- `--window`/`--depth` (10/50), `--window-memory`, `--threads` → paralelismo de delta search.
- `-b`/`--write-bitmap-index`.
- `--filter=<spec>` + `--filter-to=<dir>`: segrega objetos (ex.: LFS-like) em pack separado.

## 5. Partial clone / Sparse checkout (repos gigantes)

```bash
git clone --filter=blob:none        # não baixa blobs até necessário (lazy fetch)
git clone --filter=blob:limit=1m    # filtra blobs grandes
git clone --filter=auto             # filtro sugerido pelo servidor (promisor-remote)
git sparse-checkout set <dir>       # working tree reduzido (skip-worktree)
git clone --sparse                  # só toplevel
```
- **Cone mode** (default de sparse-checkout): O(N) hash-based rápido; non-cone (padrões
  gitignore) é O(N*M) e deprecated.
- `--sparse-index`: index esparso (entradas "sparse directory") → ganho em `status`/`add`.
- Promisor remotes: packs `.promisor`; objetos ausentes buscados on-demand.

## 6. Bundles (transfer offline / air-gapped / backup)

```bash
git bundle create backup.bundle --all        # backup completo de refs
git bundle verify backup.bundle              # checa prereqs no destino
git bundle list-heads backup.bundle          # como ls-remote
git bundle unbundle backup.bundle            # ou: git fetch / git clone <file> <dir>
git bundle create inc.bundle last..master    # incremental
```

## 7. Shallow / low-depth

```bash
git clone --depth <n>                # história truncada (implica --single-branch)
git fetch --depth <n> / --deepen=<n>
git clone --shallow-since=<date> / --shallow-exclude=<ref>
```
Limitações: shallow **atrapalha** `git bisect` e `git blame` (história curta) e merge/revert
de commits fora do corte. Use para CI/checkout de release, não para dev pleno.

## 8. Acelerando comandos do dia a dia

`git status` (`git-status.adoc`):
- `status.showUntrackedFiles=no` (`-uno`) — não lista untracked.
- `core.untrackedCache=true` (`feature.manyFiles` já habilita) — cacheia untracked por dir.
- `core.fsmonitor=true` (built-in `git fsmonitor--daemon`) + untracked cache → evita scan
  de disco. `git fsmonitor--daemon start`.
- Maior ganho: `core.fsmonitor` + `core.untrackedCache` juntos.

`git diff`:
- `diff.relative=true` — só mudanças no diretório atual (menos ruído/custo).
- `diff.algorithm` = myers/histogram/patience/minimal; `diff.renameLimit` para rename detection.

`git log`: beneficiado por `core.commitGraph=true` + Bloom filters (`--changed-paths`).

`pack.useSparse` (default true): pack-objects caminha só árvores com objetos novos → packs
menores/rápidos em fetches pequenos.

## 9. Large files

- `core.bigFileThreshold` (512 MiB): arquivo acima disso é armazenado *deflated* em pack
  **sem delta compression** (evita uso excessivo de memória no repack).
- `.gitattributes` `filter=lfs` redireciona grandes blobs para storage externo (Git LFS).
  Os man pages citam LFS/git-annex apenas como solução externa.
- Delta islands (`pack.island`) agrupam objetos frequentemente clonados para clones mais rápidos.

## 10. Rede

- `protocol.version=2` (default) — wire protocol v2, negociação eficiente.
- `fetch.negotiationAlgorithm=skipping` (pula commits, menos round trips; `feature.experimental`
  já defaulta) | `consecutive` (default) | `noop`.
- `pack.window`/`pack.depth`/`pack.compression` (-1=zlib ~6) — trade-off speed×size.
- `pack.allowPackReuse` (true/single/multi): envia partes do pack bitmapped verbatim → menos CPU.

## Tabela — ganhos rápidos

| Alvo | Comando/Config | Efeito |
|---|---|---|
| repo grande | `git maintenance start` (`geometric`/`incremental`) | manutenção sem travar |
| log/merge | `core.commitGraph=true` + `commit-graph write --reachable --changed-paths` | reachability/Bloom rápidos |
| clone/fetch server | `repack.writeBitmaps=true` + `multi-pack-index write --bitmap` | counting instantâneo |
| repo gigante | `git clone --filter=blob:none` + `git sparse-checkout set` | menos blobs baixados |
| status lento | `core.untrackedCache=true` + `core.fsmonitor=true` | sem scan de disco |
| rede | `protocol.version=2` + `fetch.negotiationAlgorithm=skipping` | menos round trips |
| backup offline | `git bundle create/verify/unbundle` | transfer air-gapped |
