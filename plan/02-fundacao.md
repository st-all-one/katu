# E01 — Fundação: workspace, Rust 1.97.0+ e ports

> **Fase 0.** Cria o esqueleto: workspace Cargo, núcleo puro (sem terminal, `argv`, relógio ou RNG
> globais), ports determinísticos e adaptadores finos, mais o binário `katu`.
>
> **Decisões:** DF1, DF2, DF4, DF7.
> **Aceite do épico:** `make check` verde **com o job `msrv` a compilar em Rust 1.97.0 exato**.

---

## Objetivo do épico

Ter `cargo build` verde, lints de produção limpos, o binário `katu` respondendo `--help`/`--json`,
e os **ports determinísticos** definidos — a base para testar sem tocar o sistema operacional.
**Nenhuma regra de política, nenhuma tool, nenhum provider ainda.**

---

## O ponto inflexível: Rust 1.97.0+

Todos os entregáveis abaixo assumem e verificam:

- `rust-toolchain.toml` na raiz e em `fuzz/`: `channel = "1.97.0"`, `components = ["rustfmt",
  "clippy"]`, `profile = "minimal"`.
- `[workspace.package]` com `edition = "2024"` e `rust-version = "1.97"`.
- `clippy.toml` com `msrv = "1.97"` e `check-incompatible-msrv-in-tests = true`.
- CI: job `msrv` que faz `cargo build --workspace` e `cargo test --workspace` com o toolchain
  fixado em `1.97.0`, **independente** do toolchain do runner.
- Nenhuma dependência com MSRV > 1.97 (`cargo update -Z minimal-versions` no job `msrv`).

---

## Tarefas

### E01-T01 ☐ Workspace e camadas
- **Objetivo:** workspace `katu` com crates `katu-core`, `katu-policy`, `katu-memory`,
  `katu-tools`, `katu-sandbox`, `katu-providers`, `katu-tui`, `katu` (binário) e `xtask`.
- **Entregáveis:** `Cargo.toml` do workspace; `src/lib.rs` de cada crate; `MODULE.md` por crate.
- **Aceite:** `cargo build` compila cada crate isolado; `katu-core`/`katu-policy`/`katu-tools`
  **não** dependem de nenhum crate de provider (firewall LLM-free, §21) — verificado por
  `xtask check-layers`.
- **Rust 1.97.0+:** `cargo build` verde no job `msrv`.

### E01-T02 ☐ Ports determinísticos
- **Objetivo:** traits `Clock`, `Rng`, `Fs`, `Env`, `Logger` no núcleo; impls reais nos
  adaptadores; fakes no núcleo para teste.
- **Entregáveis:** traits + impls; fakes (`FixedClock`, `SeqRng`, `MemFs`, `NullLogger`).
- **Aceite:** o núcleo compila sem dependências de SO/terminal; um teste do núcleo usa só fakes e
  é reprodutível byte a byte; nenhuma chamada a `SystemTime::now()`/`HashMap`-sem-ordem fora dos
  ports (reforçado por `disallowed-methods` no `clippy.toml`).

### E01-T03 ☐ Gate de qualidade, perfil e supply chain
- **Objetivo:** travar estilo, disciplina e cadeia de dependências antes de crescer o código.
- **Entregáveis:** `rustfmt.toml` (`edition = "2024"`, `max_width = 100`); `clippy.toml` (rigor
  máximo, `msrv = "1.97"`); `[workspace.lints]` com `unsafe_code = "deny"`, `unwrap_used`,
  `expect_used`, `panic`, `dbg_macro`, `print_stdout`, `print_stderr`, `exit`,
  `undocumented_unsafe_blocks`, `as_conversions` = `deny`; `[profile.release]`
  (`lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `overflow-checks = true`,
  `panic = "abort"`); `Cargo.lock` commitado; `publish = false`; `scripts/check_file_length.sh`
  (≤ 300 linhas de produção); alvo `make check`; `cargo denial`/`cargo tree` sem runtime pesado.
- **Aceite:** `make check` roda `fmt --check` + `clippy --all-targets -D warnings` + `test` +
  gate de linhas + `cargo tree`; `clippy.toml` aplicado.
- **Rust 1.97.0+:** `clippy.toml` declara `msrv = "1.97"`; o gate roda também no job `msrv`.

### E01-T04 ☐ Esqueleto do binário `katu`
- **Objetivo:** `main.rs` mínimo com parsing de subcomandos (stub), envelope `--json`
  (`{success, command, error}`), exit codes e EPIPE → exit 0.
- **Entregáveis:** `katu` com `clap`; tratamento de pipe fechado.
- **Aceite:** `katu --help`; `katu --json` devolve envelope; `katu ... | head -1` sai com 0.

### E01-T05 ☐ Documento de arquitetura e grafo de camadas
- **Objetivo:** registar a separação kernel/política/adaptadores e o grafo de dependências.
- **Entregáveis:** `ARCHITECTURE.md` (camadas + ports + mapa de módulos); `layers.toml` (equivalente
  ao `tach.toml` do docling, §36) com `depends_on` permitidas por crate.
- **Aceite:** `xtask check-layers` falha se `katu-core` importar `katu-providers`, se
  `katu-policy` importar `katu-tui`, etc.; `xtask check-crate-coverage` falha se algum módulo
  ficar fora das camadas.

### E01-T06 ☐ Modelo de erro e envelope de máquina
- **Objetivo:** definir a taxonomia de erro **antes** de a espalhar pelo código (cf. `ToolOutcome`
  do docling, §38).
- **Entregáveis:** `enum Error` no núcleo com `thiserror` (`#[from]`, `#[source]`),
  `#[non_exhaustive]`, `Send + Sync + 'static`; `ErrorKind` estável (`not_found`, `invalid_input`,
  `conflict`, `io`, `timeout`, `config`, `schema`, `unsafe_blocked`, `unavailable`, `internal`);
  helper de poison (`PoisonError::into_inner` + warn); `anyhow` + `with_context` na CLI; mapa
  **código → exit code**; `ToolOutcome = Ok | Partial | Denied | Timeout | Unavailable` com
  `OutcomeError{component, category, scope, message}` (§38).
- **Aceite:** `source()` encadeia; nada de `Box<dyn Error>` na API do núcleo; todo erro de I/O
  carrega `path`/`id`; a invariante "nunca `Ok` com erros pendurados" tem teste.

### E01-T07 ☐ Logging, observabilidade e redação
- **Objetivo:** logs úteis que **nunca** quebram o pipe nem vazam segredo.
- **Entregáveis:** impl do port `Logger` com `tracing` + `tracing-subscriber` (`EnvFilter`);
  regra **stdout = dados / stderr = logs**; níveis documentados; `#[instrument]` nas operações;
  campos estruturados; redação por allowlist (corpos, `[secrets]`, `Authorization`, tokens);
  `session_id` como span raiz; log em ficheiro opcional com rotação e teto.
- **Aceite:** `katu … --json 2>/dev/null` é JSON válido; segredo plantado nunca aparece no log;
  teste de redação.

### E01-T08 ☐ Política de memória e `unsafe`
- **Objetivo:** manter a segurança de memória por construção.
- **Entregáveis:** `#![forbid(unsafe_code)]` nos crates puros; `unsafe` só no sandbox/FFI com
  `#[allow(unsafe_code)]` + `// SAFETY:`; proibir `Rc`/`RefCell` no núcleo (`disallowed_types`);
  `try_reserve`/`Cow<'_, str>` onde couber; `O_NOFOLLOW`/canonicalização ao abrir ficheiros;
  `Drop` determinístico (sem `mem::forget`).
- **Aceite:** compila com `forbid(unsafe_code)`; Miri verde (E13); zero `unsafe` fora do
  sandbox; symlink rejeitado.

### E01-T09 ☐ Política de recursos e runtime mínimo
- **Objetivo:** teto de memória/disco/tempo, sem runtime pesado.
- **Entregáveis:** canal bounded + backpressure; pool limitado a `available_parallelism()`;
  timeouts tipados e retry/backoff só em operação idempotente; cap de corpo e de cache; decisão de
  runtime (**worker bloqueante por padrão**; `tokio` mínimo só se necessário); `spawn_blocking`
  para o núcleo puro síncrono do knudge (ver [`04`](04-contrato-da-porta-memory.md)).
- **Aceite:** `cargo tree` sem `tokio full`; rajada acima do teto não estoura memória; I/O lento
  não trava o comando.

### E01-T10 ☐ `xtask` e CI em camadas
- **Objetivo:** um único ponto de entrada para os gates, local e em CI (o `extension_cli` do zed,
  §57.2).
- **Entregáveis:** `cargo xtask` com `check` (fmt+clippy+test+linhas+layers), `check-layers`,
  `check-crate-coverage`, `check-docs`, `msrv`; workflows `pr-fast` (fmt/clippy/layers),
  `pr-msrv` (Rust 1.97.0), `ci` (test/bench).
- **Aceite:** `cargo xtask check` reproduz o CI localmente; o job `msrv` usa `1.97.0` exato;
  `CI` não depende de configuração implícita do runner.

---

## Definition of Done

- [ ] E01-T01…T10 concluídas e aceites verdes.
- [ ] `cargo xtask check` verde do zero.
- [ ] job `msrv` verde em Rust 1.97.0.
- [ ] `ARCHITECTURE.md`, `layers.toml`, `MODULE.md` presentes.
- [ ] Modelo de erro, política de log e de memória publicados.

## Não-objetivos

- Nada de política real, tools, provider, memória ou TUI (vêm em E02+).
- Nada de FFI/WASM (E11, e mesmo aí não no MVP).
