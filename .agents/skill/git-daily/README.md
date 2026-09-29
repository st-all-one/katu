# Dossiê: Uso Efetivo do Git no Workflow de Desenvolvimento

Dossiê objetivo e denso, derivado da documentação oficial do Git (`git_docs/`,
man pages em asciidoc), organizado em torno de quatro pilares de engenharia:

- **Autenticidade** — proveniência e integridade criptográfica de commits, tags e pushes.
- **Segurança** — credenciais, segredos, vazamento de dados em transferência e posse de repositórios.
- **Performance** — higiene de objetos, packing, partial clone e aceleração de comandos do dia a dia.
- **Manutenibilidade** — disciplina de commits, bisect/blame, hooks e recuperação.

Cada seção cita o arquivo de origem (`git_*.adoc`) para rastreabilidade. Comandos,
flags e chaves de configuração são preservados em inglês; a prosa está em português.

## Índice

| Arquivo | Conteúdo |
|---|---|
| `01-fluxo.md` | Fluxo de trabalho: branches de tópico, "merge upwards", publicação e integração. |
| `02-autenticidade.md` | Assinatura de commits/tags (GPG/SSH/X.509), verificação e signed pushes. |
| `03-seguranca.md` | Credential helpers, segredos, `transfer.fsckObjects`, `safe.directory`, transporte. |
| `04-performance.md` | `git gc`/`maintenance`, commit-graph, packs/MIDX, partial clone, sparse, bundles. |
| `05-manutenibilidade.md` | Commits atômicos, rebase, bisect, blame, hooks, worktree, range-diff, reflog. |
| `06-cheatsheet.md` | Referência rápida: sintaxe de revisões e comandos essenciais. |
| `07-workflow-otimizado.md` | Resumo de um workflow padrão que integra os quatro pilares. |
| `08-aliases-e-flags.md` | Flags essenciais de add/commit/pull/push/fetch/rebase/log + bloco de aliases. |
| `.gitconfig.example` | Configuração global completa e recomendada, comentada em português. |
| `alias.zsh` | Aliases de shell zsh alinhados ao `.gitconfig` (fonte única de verdade). |
| `configs/.gitconfig-{job,org,personal}` | Templates de identidade/credencial por pasta (usados via `includeIf`). |

## Princípios transversais

1. **Commits pequenos e atômicos** são a unidade de manutenção: facilitam `bisect`, `revert`
   e revisão. (gitworkflows.adoc, SubmittingPatches)
2. **Não reescreva histórico publicado.** Rebase/interativo só em branch de tópico local.
   (git-rebase.adoc, gitworkflows.adoc)
3. **Assine o que importa** e verifique antes de confiar. Assinatura é prova, não política de servidor.
   (gitformat-signature.adoc)
4. **Mantenha o repositório limpo e rápido** com `git maintenance` e commit-graph. (git-maintenance.adoc)
5. **Nunca comite segredos**; se cometer, remova com `git filter-repo` e comunique o rewrite.
   (git-filter-branch.adoc, gitcredentials.adoc)
