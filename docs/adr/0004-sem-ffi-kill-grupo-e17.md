# ADR 0004 — Sem FFI no MVP: kill do grupo de processos fica para a jail (E17)

- **Estado:** aceite
- **Data:** 2026-10-06
- **Decisões fundacionais:** DF1 (núcleo possuído), DF4 (fail-closed), G3 (superfície mínima)
- **Épicos:** E07 (E07-T04), E17 (jail futura)

---

## Contexto

O **E07-T04** pede padrões defensivos de execução, incluindo "o `timeout` mata o grupo inteiro". O
`StdProcess` já põe o filho num **process group** próprio (`process_group(0)`, API **segura** do
`std`) e, no timeout, mata e reaproveita o filho direto (sem zombie). Matar o **grupo** (netos) exige
`killpg(2)` — sem API segura no `std`, logo `rustix` ou `libc`.

`crates/katu` tem `#![forbid(unsafe_code)]` e **nenhum** dos dois está na árvore de dependências
(`cargo tree -i libc` / `-i rustix` vazios). A contenção real é a **jail (E17)**, onde
cgroups/namespaces supervisionam a árvore inteira.

## Decisão

**Não** se adiciona FFI (`libc`/`rustix`) no MVP; `#![forbid(unsafe_code)]` mantém-se. O kill do
grupo e a supervisão de processos passam para a **jail real (E17)**, onde a decisão de dependência
será tomada uma única vez. Sem FFI, o `StdProcess` mitiga o risco de *liveness* limitando a leitura
de `stdout`/`stderr` (`READ_GRACE_MS`): um neto que segure o pipe deixa de bloquear o loop.

## Alternatives considered

1. **Adicionar `rustix` (API segura) e `killpg`.** Rejeitada: o `bash` continua **correto** sem o
   kill do grupo (é robustez, não segurança); o custo de superfície de uma dependência FFI não é
   justificado por um consumidor atual (filtro `00b` §4 / anti-YAGNI). É a decisão natural **dentro**
   de E17, com a jail a dar-lhe contexto.
2. **Adicionar `libc` e escrever `unsafe`.** Rejeitada: quebra `#![forbid(unsafe_code)]` e é mais
   perigosa que a API segura do `rustix`.
3. **Invocar o utilitário `kill` (PID negativo).** Rejeitada: depende do `PATH`/`/bin/kill` e spawna
   um processo por timeout — frágil e não determinístico.
4. **Diretório de *scratch* privado `0700`.** Rejeitada/*superseded*: o temporário da escrita atómica
   **tem** de ficar no mesmo diretório do alvo (rename atómico no mesmo FS); o endurecimento vem de
   `O_EXCL` + `0600` + nome imprevisível, não do diretório.

## Consequências

- **Positivas:** `#![forbid(unsafe_code)]` preservado; **zero** dependências novas; o loop não
  bloqueia num neto que segure o pipe; o filho é sempre reaproveitado (sem zombie); os outcomes
  continuam ortogonais (`exit_code`/`signal`/`timed_out`).
- **Negativas / dívida:** um comando que deixe netos pode deixá-los a correr até E17; os leitores
  destacados podem ficar pendurados até o pipe fechar (uma thread + fd por timeout).
- **Travas:** `cargo tree -i libc`/`-i rustix` vazios; testes `normal_command_returns_stdout`,
  `timeout_kills_the_direct_child`, `missing_program_is_not_found`;
  `atomic_write_refuses_a_planted_symlink`.
