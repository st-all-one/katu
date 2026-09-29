# E07 — Sandbox e fail-closed

> **Fase 3.** A contenção real de execução (DF4). A negação in-process é semântica; um comando
> pode escapar. Aqui fecha-se o segundo lado do bloqueio (§4 da brainstorm).
>
> **Decisões:** DF4. **Depende de:** E05 (e idealmente E06).
> **Gate do épico:** **nunca passthrough não-confinado silencioso**; fiscalização relatada.

---

## Modelo (do `dsh`, §43)

```rust
pub enum SandboxMode { ReadOnly, WorkspaceWrite, FullAccess }
pub enum SandboxEnforcement { Full, Partial }
```

Regras:

1. **Só `ReadOnly` e `WorkspaceWrite` chegam a um provider.** `FullAccess` exige autorização
   explícita e registada — nunca um banner (`--host` do maxima, §49.7).
2. **A fiscalização é um facto relatado, não uma promessa.** `Partial` = um backend ou ABI de
   kernel antigo governa só um subconjunto; quem exige garantia absoluta tem de rejeitar.
3. **A política é por chamada**, não fixada no provider; um retry escalado aprovado é uma **nova**
   chamada com política mais larga.
4. **`denialSignatures` são o dialeto do backend** (EROFS no bwrap, EACCES no Landlock, EPERM no
   Seatbelt) — usar a **união** só entre backends efetivamente presentes.
5. **`RunnerFailureRule` exige conjunção**: exit ≠ 0 **e** assinatura fatal numa linha de stderr,
   após remover por igualdade exata as linhas informativas. Exit sozinho nunca prova falha.

---

## Tarefas

### E07-T01 ☐ Port de sandbox e modos
- **Entregáveis:** `Sandbox` port; mapeamento `SandboxMode` → política; `FullAccess` fora do port.
- **Aceite:** pedir `FullAccess` sem autorização devolve `NeedsHuman{missing_control}`; nenhum
  caminho faz execução livre implícita.

### E07-T02 ☐ Backend Linux (landlock/seccomp) primeiro
- **Entregáveis:** implementação para Linux com relatório de fiscalização (`full`/`partial`).
- **Aceite:** sob política confinada, escrita fora do workspace falha; o relatório diz `full` ou
  `partial` conforme o ABI detetado.

### E07-T03 ☐ **Gate do épico:** indisponibilidade = recusa
- **Objetivo:** provar fail-closed de verdade.
- **Entregáveis:** teste que força `SandboxUnavailable` (ex.: Landlock indisponível).
- **Aceite (gate):** com o controlo ausente, o comando **não** roda; resultado `Unavailable` com o
  `ControlId` exato. Uma política confinada **nunca** cai para execução livre.

### E07-T04 ☐ `RunnerFailureRule` conjuntivo
- **Entregáveis:** análise de stderr que exige conjunção + exclusões exatas; desconhecido =
  falha-fechada (§43, postmortem 0004).
- **Aceite:** um aviso benigno com prefixo partilhado **não** é confundido com falha; teste com o
  caso real do prefixo `landlock-run:`.

### E07-T05 ☐ Path jail e inspetor de argv
- **Entregáveis:** jail por `realpath`/`openat2`-style (bloqueia symlink escape); inspetor de argv
  (`interpreter -c/-e` = shell com passos extra); timeout de wall-clock com kill do **process
  group**.
- **Aceite:** symlink para fora é negado; `timeout` mata o grupo inteiro; `SandboxResult` é
  estruturado (`denied`/`timed_out`/`truncated` + razão).

### E07-T06 ☐ Padrões defensivos obrigatórios
- **Entregáveis:** outcomes ortogonais (`timedOut`/`signal`/`exitCode` independentes); dispose
  atinge quiescência (fechar listeners antes de matar, esperar filhos); scrub de env
  (`*KEY*`/`*SECRET*`/`*TOKEN*`/`*PASSWORD*`); ficheiros temporários em diretório privado `0700`,
  nomes aleatórios, abertura exclusiva `wx`/`0600`; unlink de links (`lstat`).
- **Aceite:** cada padrão tem teste próprio; env com segredo plantado não chega ao filho (§43.6).

---

## Definition of Done

- [ ] E07-T01…T06 concluídas.
- [ ] Zero passthrough não-confinado silencioso (teste).
- [ ] Fiscalização relatada (`full`/`partial`) exposta na UI e nos logs.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- macOS/Windows: fallback **degradado explícito** (não fingir paridade). Registar como dívida.
- Container/Docker como requisito: o sandbox é de SO, não um daemon (§51.12).
