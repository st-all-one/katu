# Catálogo (gerado)

> **Gerado** por `cargo xtask check-catalog`; não editar à mão. Regenerar: `KATU_GEN_DOCS=1 cargo run -p xtask -- check-catalog`.

## Tools (11)

| tool | descrição |
| --- | --- |
| `read` | Use when you need file contents or structure. Do not use for searching many files (use grep or find). |
| `write` | Use when creating a brand-new file. Do not use for changing an existing file (use edit). |
| `edit` | Use when changing an existing file. Do not use for new files (use write). One call may carry several replacements, applied in order and all-or-nothing. |
| `move` | Use when renaming or relocating a file. Do not use for copying contents between files. |
| `trash` | Use when removing a file recoverably. Do not use for permanent deletion. |
| `bash` | Use when running a program with known argv. Do not use for evaluating a shell string. |
| `grep` | Use when searching the contents of many files. Do not use for filenames (use find). |
| `find` | Use when searching for filenames. Do not use for file contents (use grep). |
| `ls` | Use when mapping a directory's languages and exports. Do not use for reading file contents (use read). |
| `plan` | Use when recording the task plan artifact. Do not use for long-term memory (use memory). |
| `memory` | Use when explicitly recording or recalling long-term memory. Do not use for ordinary file edits. |

## Regras (8)

| id | categoria | enunciado | remédio (Q-08) |
| --- | --- | --- | --- |
| `contain-read-outside-workspace` | enforced | Ler fora do workspace exige aprovação | leia dentro do workspace, ou peça aprovação explícita (`--approve`) para este caminho |
| `contain-sensitive-read` | enforced | Caminhos sensíveis (chaves/segredos) são negados por default e exigem autorização explícita | use um caminho fora da lista sensível, ou peça autorização explícita (`--approve`) ao utilizador |
| `contain-write-outside-workspace` | enforced | Escrever fora do workspace exige aprovação | escreva dentro do workspace, ou peça aprovação explícita (`--approve`) para este caminho |
| `mem-anchor-required` | enforced | Memória: nota sobre código exige `--anchor` | acrescente `--anchor ficheiro:símbolo` à nota |
| `mem-no-duplicate` | enforced | Memória: não gravar nota que seja duplicata forte (≥ 0.92) de nota existente | mude o facto ou a âncora da nota; se a nota já existe, atualize-a em vez de duplicar |
| `mem-outcome-before-close` | enforced | Memória: fechar tarefa exige `outcome` registado | registre `memory_outcome` antes de fechar a tarefa |
| `mem-recall-before-write` | enforced | Memória: não gravar nota sem ter consultado o conhecimento (recall) na mesma fase | chame `memory_recall` no mesmo turno antes de `memory_write` |
| `mem-single-claim` | enforced | Memória: uma afirmação por nota | divida em várias notas, uma afirmação por nota |

## Eventos de diag (122)

- `katu.run`
- `katu.setup`
- `katu.shutdown`
- `katu.fn`
- `fs.read`
- `fs.write`
- `fs.rename`
- `fs.mkdir`
- `fs.list`
- `fs.stat`
- `fs.remove`
- `kernel.step`
- `kernel.turn`
- `kernel.transition`
- `kernel.refusal`
- `kernel.stop`
- `kernel.budget`
- `kernel.budget_refuse`
- `lock.recovered`
- `log.append`
- `log.replay`
- `policy.evaluate`
- `policy.allow`
- `policy.deny`
- `policy.waiver`
- `policy.approval`
- `tool.call`
- `tool.ok`
- `tool.error`
- `tool.exec`
- `tool.read`
- `tool.write`
- `tool.edit`
- `tool.move`
- `tool.search`
- `tool.trash`
- `tool.plan`
- `contain.check`
- `contain.deny`
- `contain.mode`
- `process.kill`
- `memory.read`
- `memory.write`
- `memory.recall`
- `memory.handoff`
- `memory.status`
- `context.build`
- `context.trim`
- `context.compact`
- `context.checkpoint`
- `verify.report`
- `verify.override`
- `scope.load`
- `scope.merge`
- `session.open`
- `session.resume`
- `session.snapshot`
- `audit.seal`
- `audit.index`
- `audit.query`
- `toon.project`
- `toon.emit`
- `model.project`
- `context.digest`
- `schema.catalog`
- `taint.spotlight`
- `cost.check`
- `cost.refuse`
- `cost.kill`
- `cost.reenable`
- `store.load`
- `store.save`
- `provider.request`
- `provider.ttft`
- `provider.chunk`
- `provider.retry`
- `provider.error`
- `provider.models`
- `provider.tier`
- `plan.mode`
- `tui.render`
- `tui.input`
- `tui.live`
- `tui.cancel`
- `tui.steer`
- `tui.approval`
- `tui.trash_empty`
- `tui.action`
- `tui.slash`
- `mouse.copy`
- `cli.prime`
- `cli.run`
- `cli.tui`
- `cli.memo`
- `cli.config`
- `cli.upgrade`
- `cli.sessions`
- `cli.input`
- `bootstrap.init`
- `bootstrap.fix`
- `config.load`
- `config.set`
- `watch.tick`
- `watch.drain`
- `skill.discover`
- `skill.read`
- `embed.request`
- `agent.route`
- `agent.loop`
- `agent.reconcile`
- `agent.turn_lock`
- `agent.stop`
- `agent.empty`
- `agent.echo`
- `agent.stall`
- `agent.voi.skip`
- `memory.open`
- `bus.publish`
- `bus.deliver`
- `policy.load`
- `policy.audit`
- `policy.capability`

## ADRs (27)

- [ADR 0001 — MVK aprovado: o loop possuído (DF1) torna-se compromisso](../adr/0001-mvk-gate-aprovado.md)
- [ADR 0002 — Ferramentas AI-first: envelope + views + TOON (core por medição)](../adr/0002-ferramentas-ai-first.md)
- [ADR 0003 — Vocabulário de política v2: leitura sensível e acesso fora do workspace](../adr/0003-vocabulario-v2-contencao.md)
- [ADR 0004 — Sem FFI no MVP: kill do grupo de processos fica para a jail (E17)](../adr/0004-sem-ffi-kill-grupo-e17.md)
- [ADR 0005 — Formato ao modelo: colunar D39 (header autodescritivo, `\x1f`)](../adr/0005-formato-colunar-d39.md)
- [ADR 0006 — TOON colunar v3: sem headers, blocos literais e aliases de sessão](../adr/0006-toon-colunar-v3.md)
- [ADR 0007 — Projeções model-facing, digest `m`, catálogo de tools e emissor direto](../adr/0007-projecoes-model-facing.md)
- [ADR 0008 — Sessões: identidade, vinculação ao projeto, snapshot e retomada](../adr/0008-sessoes-identidade-e-retomada.md)
- [ADR 0009 — Auditoria densa e pesquisável (`.katu/audit`, completa sob compactação)](../adr/0009-auditoria-densa.md)
- [ADR 0010 — Memória de primeira classe e substituível (porta `Memory` + adaptador knudge)](../adr/0010-porta-memory-e-substituibilidade.md)
- [ADR 0011 — Porta `Provider` e built-in `opencode go/zen` sobre transporte bloqueante](../adr/0011-porta-provider-e-builtin-opencode.md)
- [ADR 0012 — Catálogo de dialetos e providers declarativos](../adr/0012-catalogo-dialetos-e-providers-declarativos.md)
- [ADR 0013 — Cache de prefixo por modelo e compressão de pedido (medida)](../adr/0013-cache-de-prefixo-e-compressao-de-pedido.md)
- [ADR 0014 — Orçamento de latência do provider: gate offline determinístico](../adr/0014-orcamento-de-latencia-do-provider.md)
- [ADR 0015 — Loop de turnos e roteador de tool calls](../adr/0015-loop-de-turnos-e-roteador.md)
- [ADR 0016 — Política de memória e `unsafe`](../adr/0016-politica-de-memoria-e-unsafe.md)
- [ADR 0017 — Política de recursos e runtime mínimo](../adr/0017-recursos-e-runtime-minimo.md)
- [ADR 0018 — Kill do grupo de processos com um único `unsafe`](../adr/0018-kill-do-grupo-com-unsafe-unico.md)
- [ADR 0019 — Superfície v2 do CLI e da TUI](../adr/0019-superficie-cli-tui-v2.md)
- [ADR 0020 — Configuração global/local e `--params`](../adr/0020-configuracao-global-local.md)
- [ADR 0021 — Layout central do `.katu/` e versionamento](../adr/0021-layout-katu-e-versionamento.md)
- [ADR 0022 — Modo de planeamento (`/plan`) e `deny_write_outside`](../adr/0022-modo-plano-e-deny-write-outside.md)
- [ADR 0023 — Contexto do projeto (`AGENTS.md`/skills) e duas IAs (embedding + execução)](../adr/0023-contexto-e-duas-ias.md)
- [0024 — Durabilidade do log: `fsync` por evento ou por turno (opt-in)](../adr/0024-durabilidade-do-log.md)
- [0025 — Decodificação estruturada por JSON Schema: opt-in, fail-open](../adr/0025-decodificacao-estruturada-por-schema.md)
- [0026 — Modo `batch` declarativo: manter o loop nativo (B-08)](../adr/0026-modo-batch-declarativo-manter-loop-nativo.md)
- [0027 — Fronteiras de transporte fechadas: SSE no provider, kernel in-process](../adr/0027-fronteiras-de-transporte.md)
