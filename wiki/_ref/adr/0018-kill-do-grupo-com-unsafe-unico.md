# ADR 0018 — Kill do grupo de processos com um único `unsafe`

- **Estado:** aceite
- **Data:** 2026-10-08
- **Decisões fundacionais:** DF4 (fail-closed), G3 (superfície mínima)
- **Épicos:** E07 (E07-T04), E13 (check-unsafe)
- **Relaciona:** [ADR 0004](0004-sem-ffi-kill-grupo-e17.md),
  [ADR 0016](0016-politica-de-memoria-e-unsafe.md), [`clippy.toml`](../../../clippy.toml)

## Contexto

A [ADR 0004](0004-sem-ffi-kill-grupo-e17.md) adiou o kill do **grupo** de processos para a jail
(E17), mantendo `#![forbid(unsafe_code)]` e **sem** `libc`/`rustix`. O `StdProcess` já cria um
process group próprio (`process_group(0)`) e mata o filho direto no timeout, mas um comando que
deixe **netos** (ex.: `sh -c 'sleep 30 & …'`) sobrevive ao timeout, pode segurar o `stdout`/fds e
escapa à higiene de recursos. É um problema de *liveness* e de execução, não de segurança de
memória; o `std` não expõe wrapper seguro para `kill(-pgid)`.

A jail real (E17) continua a ser o destino da supervisão por cgroups; esta decisão resolve apenas o
caso concreto e barato: matar a árvore que o próprio katu criou.

## Decisão

1. Adicionar `libc` **só em unix** (`[target.'cfg(unix)'.dependencies]`) ao binário `katu`.
2. Implementar um **único** ponto `unsafe` — `kill_group` em `crates/katu/src/ports/process.rs` —
   que envia `SIGKILL` ao grupo do filho (`kill(-pgid, SIGKILL)`), com
   `#[allow(unsafe_code, reason = …)]` e um `// SAFETY:` **colado** ao bloco, provando a invariante
   (o `pgid` é o líder do grupo que criámos; nunca um grupo alheio).
3. O binário `katu` declara `#![deny(unsafe_code)]` (não `forbid`) para permitir o `allow` local;
   **todos** os outros crates mantêm `#![forbid(unsafe_code)]`.
4. `xtask check-unsafe` registra a exceção numa lista versionada e exige que ela esteja
   **exatamente** esgotada (um `allow` a mais ou a menos falha o gate).

## Alternatives considered

1. **`rustix` (API segura para `killpg`).** Rejeitada: adiciona uma dependência FFI maior para um
   único `killpg`; o `libc` já está na árvore transitiva e o `unsafe` fica com fronteira explícita e
   auditável, que é o que a ADR 0016 exige.
2. **Manter a ADR 0004 (adiar para E17).** Rejeitada: o caso concreto é barato, o custo é um só
   `unsafe` isolado e o benefício (sem netos órfãos) é imediato; E17 continua a tratar da supervisão
   por cgroups.
3. **Invocar o utilitário `kill` com PID negativo.** Rejeitada (já em ADR 0004): depende do
   `PATH`/`/bin/kill`, spawna um processo por timeout e não é determinístico.
4. **`#![forbid(unsafe_code)]` no binário, sem exceção.** Rejeitada: `forbid` não é anulável; a
   alternativa seria não ter o kill do grupo, o que é o *statu quo* que esta ADR corrige.

## Consequências

- **Positivas:** no timeout o **grupo** inteiro é morto (`timeout_kills_the_process_group`), sem
  netos órfãos nem fds pendurados; o `unsafe` é **um só**, com fronteira provada e verificada por
  gate; o resto do workspace continua `forbid`.
- **Negativas / dívida:** uma dependência (`libc`, só unix) e um ponto `unsafe` a rever com cuidado;
  a supervisão completa (cgroups/namespaces) continua em E17.
- **Travas:** `xtask check-unsafe` (exceção exatamente esgotada); Miri no CI; testes
  `timeout_kills_the_direct_child`, `timeout_kills_the_process_group`, `missing_program_is_not_found`.
