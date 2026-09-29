# E07 — Contenção determinística (soft) e fail-closed

> **Fase 3.** No **MVP não há jail de SO**. O katu corre **global de facto** — vê o host como o
> utilizador que o evocou — e o que limita a IA são as **travas determinísticas**: a política de
> E02 imposta **na operação** (caminhos canonicalizados, capacidades tipadas,
> `Allow`/`Deny`/`RequireApproval`/`NeedsHuman`), a execução como o utilizador, `argv` resolvido,
> timeouts e recusa onde falta um controlo.
>
> Isto é uma barreira **soft e declarada**: **não** é fronteira de segurança contra código
> arbitrário. A **jail real** (bwrap + Landlock + seccomp) é feature futura, fora do plano
> principal — ver [`18-jail-futuro.md`](18-jail-futuro.md) e o **`ai-jail`** (pesquisa externa,
> fora do projeto).
>
> **Decisões:** DF4. **Depende de:** E05 (e idealmente E06).
> **Gate do épico:** nenhuma operação sensível passa sem veredicto; controlo em falta = recusa; a
> limitação é **declarada** (soft), nunca vendida como fronteira de segurança.

---

## Modelo

```rust
/// No MVP só existe contenção soft. `Unconfined` = global, como o utilizador.
pub enum Containment { Soft, Unconfined }

/// Nível de garantia honestamente relatado. No MVP é sempre `Soft`.
pub enum SandboxEnforcement { Soft, Full, Partial }  // Full/Partial só com a jail futura (E17)
```

**Regra zero — global por omissão, com travas.** O katu corre com os privilégios do utilizador e
acesso ao host; **não** confina por kernel. A limitação vem da política: caminhos canonicalizados,
`Capability::{ReadPath, WritePath, DeletePath, Exec, Net}`, regras determinísticas e
`RequireApproval`/`NeedsHuman` para o que sai do workspace ou toca caminhos sensíveis.

Regras:

1. **Sem jail no MVP.** Nenhuma promessa de isolamento de SO. `SandboxEnforcement` é `Soft` e a UI
   di-lo claramente; "modo sem isolamento" não é de primeira classe nem escondido num banner.
2. **A política é imposta na operação** (§45.20): quem executa consulta o veredicto; a negação
   ocorre no executor, não num wrapper.
3. **Caminhos canonicalizados antes de decidir** (`realpath`/`openat2`-style onde barato): `..` e
   symlinks resolvem-se **antes** do veredicto; o motor nunca infere acesso a partir de path relativo.
4. **Sensíveis são `deny`-by-default (soft).** `.ssh`, `.aws`, `.gnupg`, `.docker`, `.env`/segredos,
   dotfiles de credenciais: regra determinística nega por omissão; acesso exige `RequireApproval`.
5. **Fora do workspace = `RequireApproval`/`NeedsHuman`.** Ver/ler/escrever/executar fora da área de
   trabalho corrente (e qualquer `Capability::Net`) passa por **autorização explícita** no CLI/TUI —
   *soft*, registada com `override_reason`+`granted_by`. (Com a jail futura isto vira `PathGrant` de
   kernel; hoje é só política.)
6. **A execução herda o utilizador que evocou o processo.** `uid/gid` do utilizador do katu — nunca
   `sudo`/setuid, nunca elevação; se a política pedir mais do que o utilizador tem, é
   `Denied`/`Unavailable`, não escalada.
7. **`argv` resolvido e `cwd` fixado antes da política**; inspetor de `argv`
   (`interpreter -c/-e` = shell com passos extra); timeout de wall-clock com kill do **process
   group**.
8. **Controlo em falta = recusa (fail-closed).** Não consigo canonicalizar? Não sei o `argv`? Não há
   `exit_code`? Ambíguo? → `Denied`/`Unavailable`; desconhecido não executa.
9. **Honestidade obrigatória (documentada e testada).** As travas soft só vinculam as **tools do
   katu** e os comandos lançados com `argv` conhecido; `bash script.sh` (ou qualquer binário)
   continua a poder fazer tudo o que o utilizador pode. Isto é afirmado no produto, nos logs e nos
   testes — nunca omitido.

---

## Tarefas

### E07-T01 ☑ Port de contenção e modos
- **Entregáveis:** `Containment` port (`Soft`/`Unconfined`); mapeamento para a política;
  `SandboxEnforcement::Soft` sempre relatado; gancho para a jail futura (E17) sem a implementar.
- **Estado:** `katu_core::containment`: `Containment` (`Soft` por omissão; `enforces_policy`),
  `SandboxEnforcement` (`Soft`/`Full`/`Partial`, `is_kernel_isolated`) e `ContainmentStatus::mvp`.
  `announce` emite `contain.mode` (honestidade: `soft`, `kernel_isolated=false`). O gancho `Jail`
  com `NoJail`: pedir `Full`/`Partial` devolve `ContainmentError::Unavailable` (fail-closed),
  nunca execução livre.
- **Aceite:** a UI e os logs dizem `soft`; nenhum caminho promete isolamento de SO; ligar a jail
  futura sem implementação devolve `Unavailable`, não execução livre.

### E07-T02 ◐ Canonicalização, capacidades e inspetor de argv
- **Entregáveis:** resolução canónica de paths (`..`, symlink) **antes** do veredicto; inspetor de
  `argv`/`cwd`/interpretador; `Capability::Exec`; nenhuma decisão por regex sobre a string.
- **Estado:** `..`/`.` são normalizados lexicalmente em `ResolvedPath` (E02-T01). O inspetor
  `katu_policy::inspect` classifica o `argv` por igualdade de strings
  (`ProgramKind::{Command, Interpreter, InlineInterpreter}`), flags inline (`-c`/`-e`/`--eval`),
  aninhadas (`find -exec`) e destrutivas (`find -delete`), com `is_opaque`/`is_plain`.
  `Capability::Exec { program }` passa a destrancar `DenyCommand { Exec }` **só** para um `argv`
  verificável (não opaco/destrutivo) com o programa exato — `bash -c`, `find -delete`,
  `find -exec` e `r''m` continuam negados (golden E02-T05). **Falta:** resolução de **symlink** via
  porta `Fs`; o ponto de integração é a resolução do tool call (E12).
- **Aceite:** `cd x && rm`, `bash -c`, `find -delete`, `r''m` avaliados sobre factos; symlink para
  fora é negado **na política**; o golden de E02-T05 cobre estes casos.

### E07-T03 ◐ **Gate do épico:** controlo em falta = recusa + honestidade
- **Objetivo:** provar fail-closed onde há controlo, e honestidade onde não há.
- **Entregáveis:** testes de recusa (path não canonicalizável, `argv` desconhecido, `exit_code`
  `null`, autorização ausente para fora do workspace) e um teste que afirma `soft`/não-fronteira.
- **Estado:** `crates/katu-tools/tests/containment_gate.rs`: vocabulário desconhecido ⇒ `dispatch`
  falha e a tool não corre; `argv` ausente ⇒ `Unavailable` sem execução; path relativo ⇒ não
  canonicalizável; `exit_code: null` ⇒ bloqueia avançar (kernel, E06-T07); a jail futura
  (`NoJail.acquire(Full)`) falha-fechado; e o teste de **honestidade** prova que um comando fora das
  tools do katu corre (a contenção é soft). **Falta:** autorização ausente para fora do workspace
  (E07-T05).
- **Aceite (gate):** com o controlo ausente, a operação **não** corre (`Denied`/`Unavailable` com o
  `ControlId` exato); nenhuma operação sensível passa sem veredicto; os testes provam que a
  contenção é **soft** (um comando fora do controlo do katu não é detido) — a limitação fica visível.

### E07-T04 ◐ Padrões defensivos de execução
- **Entregáveis:** scrub de env (`*KEY*`/`*SECRET*`/`*TOKEN*`/`*PASSWORD*`); ficheiros temporários
  em diretório privado `0700`, nomes aleatórios, abertura exclusiva `wx`/`0600`; unlink de links
  (`lstat`); outcomes ortogonais (`timedOut`/`signal`/`exitCode` independentes); dispose atinge
  quiescência (fechar antes de matar, esperar filhos).
- **Estado:** o scrub de env (`ExecTool::scrub_env`) e os outcomes ortogonais (`exit_code`/
  `signal`/`timed_out`) já vêm de E06-T04. A escrita atómica do `StdFs` é agora **endurecida**:
  temporário exclusivo (`O_EXCL`) com `0600` e nome imprevisível (`.<pid>.<n>.tmp`) — um symlink
  plantado no caminho do temporário é **recusado** (teste `atomic_write_refuses_a_planted_symlink`),
  o que também cobre o `lstat`/não-seguir-links. **Falta:** diretório de *scratch* privado `0700`,
  kill do **process group** no timeout e quiescência do dispose (dependem de `rustix`/libc, vedado
  por `#![forbid(unsafe_code)]` — decisão de dependência pendente).
- **Aceite:** cada padrão tem teste próprio; env com segredo plantado não chega ao filho (§43.6);
  `timeout` mata o grupo inteiro.

### E07-T05 ◐ Autorização soft fora do workspace e caminhos sensíveis
- **Entregáveis:** regras determinísticas: sensíveis `deny`-by-default; acesso a path/comando fora
  do workspace → `RequireApproval`/`NeedsHuman`; autorização interativa no CLI/TUI registada com
  `override_reason`+`granted_by`; `Capability::Net` idem.
- **Estado:** a **raiz do workspace** existe no kernel (`State::workspace`, `Event::WorkspaceSet`,
  `Session::set_workspace`); `facts_from` deriva `Capability::Workspace { root }`
  (`containment::workspace_capabilities`). O vocabulário subiu a **v2** (ADR 0003):
  `Enforcement::DenyRead { root }` (destrancado por `ReadPath` **ou** `Workspace`) e
  `Enforcement::DenySensitiveRead { globs }` (só `ReadPath` explícito — o workspace **não** conta).
  `policy/containment.toml` traz `contain-sensitive-read` (critical: `.ssh`/`.env`/chaves) e
  `contain-{read,write}-outside-workspace` (warn → `RequireApproval`). Testes:
  `katu-policy/tests/containment.rs` (matriz real) e
  `pipeline::tests::sensitive_read_is_denied_even_with_a_workspace`. A busca (`grep`/`find`/`ls`)
  é agora uma tool de **leitura** para o motor (`is_read_tool` inclui `Search`) e o `ToolUse` traz a
  raiz resolvida (`search::search_use`), pelo que varrer fora do workspace pede aprovação e varrer
  um caminho sensível (`.ssh`/`.env`) é negado. `Capability::Net` está implementada:
  `argv::inspect` marca programas de rede (`curl`/`ssh`/…) e extrai o host; `is_plain()` recusa-os
  (uma capacidade por programa **não** os destranca) e `command_capability` só cede a
  `Capability::Net { host }` (`*` = qualquer). **Falta:** o fluxo interativo de autorização
  (`override_reason`+`granted_by`, CLI/TUI E10 — hoje fica `Unavailable{approval}`).
- **Aceite:** ler `.ssh`/`.env` sem autorização é `Denied`; um pedido aprovado fica no log e na UI;
  a autorização não é herdada por um comando subsequente.

---

## Definition of Done

- [ ] E07-T01…T05 concluídas.
- [ ] Nenhuma operação sensível sem veredicto; controlo em falta = recusa.
- [ ] Contenção **soft** declarada na UI, nos logs e testada (não é fronteira de segurança).
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- **Jail de SO real (bwrap/Landlock/seccomp/namespaces/egress filtrado):** feature **futura**,
  pós-MVP, fora do plano principal — ver [`18-jail-futuro.md`](18-jail-futuro.md) e o **`ai-jail`**
  (pesquisa externa, fora do projeto). No MVP **não** se implementa.
- macOS/Windows: sem fallback degradado por agora (não há jail para degradar); registar quando E17
  arrancar.
- Container/Docker como requisito: seria da jail futura, não do MVP.
