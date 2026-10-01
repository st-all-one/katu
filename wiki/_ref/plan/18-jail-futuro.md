# E17 — (FUTURO) Jail de SO real

> **Status: deferido — fora do plano principal.** Só **depois** da implementação geral do katu
> (pós-MVP). Este documento guarda o desenho para não se perder; **não** é compromisso de fase nem
> entra na Definition of Done do MVP.
>
> **Referência:** o **`ai-jail`** (pesquisa externa, fora do projeto; bubblewrap + Landlock +
> seccomp + rlimits; política de projeto monotónica; egress filtrado; suite de escapes). É a base
> do desenho.
>
> **Decisão (quando retomar):** OA13 — reusar `bwrap` como binário externo **validado**, em vez de
> reimplementar namespaces em Rust puro.
>
> **Alternância:** a jail liga/desliga por **config global** (`[jail] enabled = true|false`). Fora
> disso, ver/ler/escrever/executar **fora** da jail exige **autorização no CLI/TUI** (interativa).
> A config de projeto é **não confiável e monotónica**: só aperta, nunca concede.

---

## Porquê futuro

No MVP o katu é **global de facto** e limita a IA pelas **travas determinísticas** da política
(E02/E07) — uma barreira *soft*, declarada, que **não** é fronteira de segurança. A jail de SO
converte essa barreira soft numa **fronteira de kernel**, mas custa plataforma (bwrap + Landlock +
seccomp + rlimits, matriz de kernels/distros, escape tests). Faz-se **depois** de o produto provar
valor, não antes.

---

## Modelo (do `dsh`, §43; jail inspirada no `ai-jail`, pesquisa externa)

```rust
pub enum SandboxMode { ReadOnly, WorkspaceWrite, FullAccess }
pub enum SandboxEnforcement { Full, Partial }   // fiscalização relatada
pub enum SandboxBackend { Bwrap, Landlock, Seccomp, Rlimits }  // camadas empilhadas

/// A única raiz visível por omissão (o "jail").
pub struct WorkspaceRoot(PathBuf);

/// Concessão explícita para fora da jail — **só** autorização interativa no CLI/TUI.
pub struct PathGrant { pub path: PathBuf, pub access: AccessMode, pub from: TrustLayer }
pub enum AccessMode { Ro, Rw }
pub enum TrustLayer { CliTui, GlobalConfig, ProjectConfig }  // ProjectConfig = não confiável
```

**Regra zero — a área de trabalho é o jail.** Com a jail ligada, o processo vê **apenas** o
workspace (bind mount) e um `$HOME` tmpfs privado; o resto do host é **invisível** (namespaces) e
**negado** (allowlist Landlock). Ler/escrever/executar **fora** exige um `PathGrant` da camada
**`CliTui`** (ou o *toggle* da config global, que só liga/desliga a jail — não concede caminhos).
A política de projeto é **não confiável e monotónica**; config inválida/ilegível falha fechada.

---

## Regras

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
6. **A execução herda o utilizador que evocou o processo.** Nunca `sudo`/setuid, nunca elevação.
7. **Defesa em profundidade.** **bwrap** (namespaces mount/PID/UTS/IPC/net, bind mounts, tmpfs,
   `--die-with-parent`) → **Landlock** (allowlist de paths; sobrevive a fuga de mount-namespace;
   impõe RO no VFS, mesmo via `/proc/self/fd` ou `mmap`) → **seccomp** (sem sockets raw/packet, sem
   `TIOCSTI`) → **rlimits**. Relatório `full`/`partial`.
8. **Rede off por omissão.** Única saída possível: **egress filtrado** por allowlist de hosts (o
   endpoint do provider), via namespace de rede privado + CONNECT proxy; segredos por placeholder.
9. **A fuga é testada.** Suite de escapes (symlink, `..`, `openat2`/`open_by_handle_at`,
   bind-mount/remount, `/proc/self/fd`, `chdir`+exec, herança de fd) que fica vermelha se o jail
   abrir (§44).

---

## Tarefas (futuras — não fazem parte do MVP)

- **E17-T01** Port de sandbox e modos; `WorkspaceRoot`/`PathGrant` (só `CliTui`); `FullAccess` fora
  do port.
- **E17-T02** Backend Linux: bwrap + Landlock + seccomp + rlimits; rejeitar `bwrap` que não resolva
  para binário root-owned e não-writable. O PID namespace + `--die-with-parent` dão o **kill da
  árvore** (grupo de processos) que o MVP defere (ADR 0004).
- **E17-T03** **Gate:** camada essencial ausente → `SandboxUnavailable`, sem execução livre.
- **E17-T04** Suite de escape do workspace (kernel, não política).
- **E17-T05** Autorização explícita (CLI/TUI) + política monotónica + `--dry-run`.
- **E17-T06** Egress filtrado (rede off por omissão).
- **E17-T07** Fallbacks explícitos (macOS `sandbox-exec` degradado; bwrap ausente).
- **E17-T08** Benchmark do custo da jail (startup/turno) e relatório `full`/`partial` na UI.

---

## Critérios de retomada

- O produto geral está implementado e estável (pós-MVP).
- O custo de plataforma (bwrap/Landlock) é aceitável face ao ganho de contenção medido.
- Há um consumidor real que precise de contenção forte (ex.: executar código não confiável).

Enquanto isso, o que vale é [`08-sandbox-fail-closed.md`](08-sandbox-fail-closed.md): contenção
determinística **soft**.
