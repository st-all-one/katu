# Arquitetura do katu

> Documento de referência (E01-T05). A **fonte de verdade** do firewall de camadas é
> [`layers.toml`](layers.toml), verificado por `xtask check-layers`. O plano vive em
> [`plan/`](plan/README.md); os objetivos em [`plan/00b-objetivos.md`](plan/00b-objetivos.md).

## 1. Tese em uma linha

O katu possui o **loop** (máquina de estados + política + memória) e trata o modelo como um
**endpoint** substituível. A tese é o **enforcement**: regras determinísticas que decidem sobre
factos tipados, com evidência — não prompts.

## 2. Camadas (crates)

```
                       ┌───────────────────────────────────────────────┐
   borda / SO          │  katu (binário)                               │
   ─────────────       │   CLI · adaptadores das portas · knudge in-proc │
                       └───────┬───────────┬───────────┬───────────────┘
                               │           │           │
             ┌─────────────────┘           │           └─────────────────┐
             ▼                             ▼                             ▼
     ┌───────────────┐   ┌───────────────────────┐   ┌───────────────────────┐
     │ katu-providers│   │ katu-tools            │   │ katu-tui              │
     │ (LLM boundary)│   │ tools + contenção soft│   │ ratatui               │
     └───────┬───────┘   └───────────┬───────────┘   └───────────┬───────────┘
             │                       │                           │
             └───────────────┬───────┴───────────────┬───────────┘
                             ▼                       ▼
                     ┌───────────────┐       ┌───────────────┐
                     │ katu-core     │──────▶│ katu-policy   │
                     │ kernel + log  │       │ evaluate puro │
                     └───────────────┘       └───────────────┘
                             ▲
                             │  (firewall: puros NÃO dependem de providers/adaptadores)
                             ╳  katu-core ↛ tools/providers/tui/knudge-core
                             ╳  katu-policy ↛ core/tools/providers/tui/knudge-core
                             ╳  katu-tools ↛ providers/tui/knudge-core
```

| Crate | Papel | Não pode depender de |
|---|---|---|
| `katu-core` | kernel: máquina de estados, log append-only, portas, memória, erro, diag | tools, providers, tui, knudge-core |
| `katu-policy` | motor `evaluate` **puro**: factos tipados → veredicto | core, tools, providers, tui, knudge-core |
| `katu-tools` | tools + contenção **soft** determinística | providers, tui, knudge-core |
| `katu-providers` | fronteira LLM (built-in `opencode go/zen`, `llama.cpp`) | — |
| `katu-tui` | interface (ratatui) | — |
| `katu` (bin) | composição, CLI, adaptadores das portas, `knudge` in-process | — |
| `xtask` | gates do repositório (não é runtime) | — |

**Porquê:** o kernel e a política têm de ser testáveis sem rede, sem terminal e sem SO. O firewall
LLM-free garante que a tese (enforcement determinístico) não fica contaminada pelo modelo.

## 3. Portas (determinismo)

O núcleo nunca toca `SystemTime::now`, `std::env::var`, `HashMap` sem ordem nem o sistema de
ficheiros: atravessa uma **porta**. As *fakes* vivem em `katu-core::ports`; os adaptadores reais,
no binário.

| Porta (`katu_core::ports`) | Fake | Adaptador (`katu/src/ports.rs`) |
|---|---|---|
| `Clock` (`Timestamp`) | `FixedClock` | `SystemClock` |
| `Rng` | `SeqRng` | `StdRng` (splitmix64) |
| `Fs` | `MemFs` | `StdFs` (escrita atómica) |
| `Env` | `FakeEnv` | `StdEnv` |

Consequência: os testes do núcleo são **reprodutíveis byte a byte**; nada de `unwrap`/`panic`.

## 4. Diagnóstico transversal (DF9/E19)

`katu-core::diag` é a **única** superfície de log e métrica:

- **Logs sempre estruturados**: identificador estável do catálogo `diag::events` + campos tipados;
  nunca texto livre. `xtask check-diag` falha se `crates/*/src` usar macros de saída de texto fora
  do sink (`crates/katu/src/diag.rs`).
- **Custo zero por defeito**: sem `feature = "instrument"`, `span!`/`event!` são *no-op*.
- **On-demand**: com a feature, liga-se em runtime (`KATU_INSTRUMENT=1`).
- **Nunca no plano de dados**: não entra no log de sessão nem no contexto do modelo.

Regra em todo o código:

```rust
let _span = katu_core::span!(Level::Info, events::KERNEL_STEP, "n" => n);
katu_core::event!(Level::Debug, events::POLICY_EVALUATE, "ok" => true);
```

## 5. Modelo de erro (E01-T06)

`katu_core::error` define a taxonomia **estável** antes de a espalhar: `Error` (encadeável,
`#[source]`), `ErrorKind` (contrato de máquina: `as_str()` + `exit_code()`) e `ToolOutcome`
(`Ok | Partial | Denied | Timeout | Unavailable`). Regras: sem `Box<dyn Error>` na API do núcleo;
todo erro de I/O carrega `path`; `lock_recover` trata *poison* sem propagar `panic`.

## 6. Fluxo de dados (destino)

```
utilizador ─▶ katu (CLI/TUI) ─▶ katu-core (loop) ─▶ katu-policy.evaluate
                                      │                       │
                                      │                Decision (Allow/Deny/…)
                                      ▼
                              katu-tools (executa, com contenção soft)
                                      │
                                      ▼
                         porta Memory (knudge in-process / fake)
```

`stdout` = **dados**; `stderr` = **logs**. O envelope `--json`
(`{success, command, error?, data?}`) é a superfície estável de máquina; EPIPE é sucesso.

## 7. Gates do repositório

`make check` = `fmt` · `clippy -D warnings` · `test` · `layers` · `diag` · `docs` · `policy` ·
`bench` · `file-length`. Extras: `make instrument` (feature ligada), `make measure` (artefacto de
medição do MVK), `make deny/audit/machete/typos/miri`. CI: `.github/workflows/` (`pr-fast`,
`pr-msrv` em Rust 1.97.0, `ci`).

- **Decisões:** [`docs/adr/`](docs/adr/README.md) (ADRs com `## Alternatives considered`,
  verificadas por `xtask check-docs`); as fundacionais em
  [`plan/01`](plan/01-decisoes-fundacionais.md). O gate do MVK foi assinado na
  [ADR 0001](docs/adr/0001-mvk-gate-aprovado.md).
- **Números:** nenhum valor publicado sem base e artefacto (`xtask gate:bench`, DF5).
