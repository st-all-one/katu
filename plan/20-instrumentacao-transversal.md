# E19 — Instrumentação transversal (logs estruturados + métricas de tempo)

> **Transversal — não é fase.** Regra de projeto: **toda** operação do katu abre um `span!` de log
> estruturado e métrica de tempo, **on-demand** e de **custo zero por defeito**. Existe para que
> possamos medir cada ponto mínimo do projeto e evitar otimizações cegas e falsos positivos.
>
> **Decisões:** DF9 (e alimenta DF5/E15/E18). **Depende de:** E01.
> **Gate do épico:** desligada, **zero** registos e **zero** custo (o caminho ativo não é
> compilado); ligada, todo span tem início, fim e duração.
>
> **Fonte de método:** o exemplo do `knudge`
> ([`knudge/plan/implementation/19_performance_reforma_cli.md`](../knudge/plan/implementation/19_performance_reforma_cli.md))
> — medir antes, A/B, adotar-ou-reverter.

---

## 1. Princípio

- **Logs sempre estruturados.** Um registo é um **identificador estável** (`policy.deny`,
  `tool.call`, `provider.ttft`) + **campos tipados**; **nunca** texto livre interpolado. Todo o
  dado variável vai para campos.
- **Métrica de tempo integrada.** Cada `span!` regista início e fim com duração em nanossegundos.
- **Custo zero por defeito.** Sem `feature = "instrument"`, `span!`/`event!` são *no-op* e o caminho
  ativo **não existe** no binário. Sem a flag de runtime, não altera bytes, não faz I/O, não
  bloqueia.
- **On-demand.** Com a feature, liga-se em runtime (`KATU_INSTRUMENT=1`) e afina-se o nível
  (`set_level`); desligado, é uma leitura atómica *relaxed* + ramo.
- **Diagnóstico, não plano de dados.** Nunca entra no log de sessão nem no contexto do modelo
  (`Model-visible ⟺ logged` intacto).

## 2. Contrato (`katu-core::diag`)

```rust
pub enum Level { Error, Warn, Info, Debug, Trace }   // thresholds
pub enum Value<'a> { Str(&'a str), Int(i64), Uint(u64), Bool(bool) }
pub enum Kind { Event, SpanStart, SpanEnd }

pub struct Record<'a> {
    pub level: Level,
    pub event: &'static str,
    pub kind: Kind,
    pub duration_nanos: Option<u128>,
    pub fields: &'a [(&'static str, Value<'a>)],
}

pub trait Sink: Send + Sync { fn record(&self, record: &Record<'_>); }
```

Instalação (borda, no binário): `install(Arc<dyn Sink>)`; controlo: `set_enabled(bool)`,
`set_level(Level)`; leitura: `enabled()`, `current_level()`.

**Uso padrão** (a regra para todo o código):

```rust
use katu_core::diag::{Level, events};

fn operacao(n: usize) -> Result<(), Error> {
    let _span = katu_core::span!(Level::Info, events::KERNEL_STEP, "n" => n);
    katu_core::event!(Level::Debug, events::POLICY_EVALUATE, "ok" => true);
    // …
    Ok(())
}
```

- Identificadores **estáveis** e **namespaced** (`<subsistema>.<ação>`), nunca mensagens.
- A duração vive no `Kind::SpanEnd`; agrega-se fora do processo (p50/p95/p99), sem tocar no código.

### 2.1 Catálogo de eventos (`katu_core::diag::events`)

**Fonte única de verdade** dos identificadores (`CATALOG_VERSION = 1`, `ALL`), validada por teste
(formato `<subsistema>.<ação>`, sem duplicados). Cobre execução/fs, kernel/log, política, tools e
contenção, memória, contexto, store, providers e TUI. Mudar um id é decisão registada.

> **Camadas sem acesso a `katu-core`** (ex.: `katu-policy`, por firewall/E01) são instrumentadas
> **pelo chamador**: o kernel abre o span `policy.evaluate` em torno da avaliação. As funções
> continuam puras e silenciosas — o orquestrador mede.

### 2.2 Gate: `xtask check-diag`

O clippy (`print_stdout`/`print_stderr`/`dbg_macro`) é a primeira linha; o gate é **duro** contra
`#[allow]`: falha se `crates/*/src` usar `eprintln!`/`eprint!`/`println!`/`print!`/`dbg!` fora do
sink (`crates/katu/src/diag.rs`). Corre em `make check`.

## 3. Como medir sem interferir

- **Recorte micro + e2e** (lição do `knudge`): os ganhos podem existir só numa escala. O sink
  agregador (§tarefas) dá histogramas por `event`; o E15 cruza com o e2e.
- **Determinismo**: o caminho de diagnóstico usa um relógio monotónico **local** e não escreve
  estado observável. O resultado do programa é idêntico com a instrumentação ligada ou desligada —
  e isso é um teste.
- **Consistência**: um `fingerprint` determinístico por operação permite detetar divergência entre
  execuções (mesma entrada → mesma impressão) — o par da métrica de tempo.

## Tarefas

### E19-T01 ☑ Mecanismo base: porta `Sink` + macros + feature + catálogo
- **Entregáveis:** `katu-core::diag` (`Level`, `Value`, `Kind`, `Record`, `Sink`, `Span`); macros
  `span!`/`event!`; catálogo `diag::events` (validado por teste); `feature = "instrument"` (off por
  defeito); adapter de `stderr` no binário (`--features profile`); `make instrument`.
- **Aceite:** ligada, spans/eventos são registados (teste); desligada, `Span` é *zero-sized* e
  `enabled() == false`; build por defeito **não** compila `diag::active`; catálogo bem-formado.
- **Estado:** implementado (`diag/mod.rs`, `diag/active.rs`, `diag/disabled.rs`, `diag/events.rs`);
  entrada do binário instrumentada (`katu.run`, `katu.setup`, `fs.*`).

### E19-T02 ☑ Sink agregador (histogramas) + dump
- **Entregáveis:** sink que agrega contagens e durações por `event` (min/p50/p95/p99/max) e
  descarrega em `.katu/diag/*.jsonl` (ou `stderr`) sob pedido; sem alocação no caminho quente
  além do registo.
- **Estado:** `katu_core::diag::aggregate::AggregatingSink` (feature `instrument`): agrega só
  `SpanEnd`, percentis por *nearest-rank* em pontos base (sem vírgula flutuante), instantâneo
  determinístico por ordem canónica de `event`. O harness `measure_mvk` grava o dump como
  `bench/mvk/raw.json`.
- **Aceite:** os números saem com base tipada (DF5) e artefacto; o dump é determinístico por ordem
  canônica de `event`.

### E19-T03 ◐ Instrumentar o caminho crítico por épico
- **Entregáveis:** `span!` em cada operação do kernel (E04), política (E02 — **pelo chamador**),
  tools/contenção (E06/E07), memória (E03), contexto (E09) e providers (E12); ids do catálogo,
  campos tipados.
- **Aceite:** `xtask check-diag` verde (sem texto livre); todo `span!` usa id do catálogo.
- **Estado:** entrada, portas `fs`, kernel (E04) e o gate de memória (E05) instrumentados:
  `katu.run`/`katu.setup`, `fs.*`, `kernel.step`/`kernel.transition`/`kernel.refusal`,
  `log.append`/`log.replay`, `policy.evaluate`/`policy.allow`/`policy.deny`,
  `tool.call`/`tool.ok`/`tool.error`, `tool.read`/`tool.write`/`tool.edit`/`tool.move`/
  `tool.search`/`tool.exec`/`tool.trash`/`tool.plan`, `fs.rename`/`fs.mkdir`, `memory.write`,
  `kernel.budget[_refuse]`, `context.checkpoint`, `context.build` (E09-T01),
  `verify.report` (E09-T03), `lock.recovered`, `contain.mode`. Dos 56 ids, 35 estão emitidos;
  restam 21 por nascer (memória E03, contexto E09, providers E12, TUI E11).

### E19-T04 ☐ Consistência (fingerprint determinístico)
- **Entregáveis**: `diag::fingerprint!` que acumula um hash determinístico do estado/resultado de
  uma operação; comparável entre execuções.
- **Aceite:** mesma entrada → mesma impressão; o teste falha se a impressão mudar sem mudança de
  código (deteta não-determinismo).

### E19-T05 ◐ Gate no CI
- **Entregáveis:** `make instrument` no CI (clippy + testes com a feature); `xtask check-diag` em
  `make check`.
- **Aceite:** `clippy -D warnings` verde com `instrument`/`profile`; "zero quando desligado" e
  "regista quando ligado"; `check-diag` falha com sonda injetada.
- **Estado:** `make instrument` existe e está ligado ao CI (`ci.yml`: clippy+test com
  `--features instrument`/`profile`); `check-diag` corre em `pr-fast.yml` e em `make check`.

### E19-T06 ☐ Filtro de nível por subsistema (OA18)
- **Entregáveis:** além do nível global, ligar/desligar por prefixo de `event` (ex.: só
  `provider.*`) via ambiente; custo zero por defeito mantido.
- **Aceite:** ligar um prefixo não emite eventos de outros subsistemas; desligado continua sem
  qualquer emissão.

## Definition of Done

- [ ] Toda operação de produção abre um `span!` com nome estável.
- [ ] Logs **só** estruturados (identificador + campos); zero texto livre.
- [x] Custo zero por defeito provado por build (sem `diag::active`) e por teste.
- [x] Catálogo de eventos único e validado; logs **só** estruturados (gate `check-diag`).
- [ ] `make check` e `make instrument` verdes; `xtask check` verde em Rust 1.97.0.

## Não-objetivos

- Instrumentação que altera o plano de dados (log de sessão/contexto do modelo).
- Custo no caminho quente quando desligada (a feature **não** existe no binário por defeito).
- Recolha de segredos: campos nunca carregam corpos/credenciais (redação no sink, E01-T07).
- Substituir o E15/E18: E19 **instrumenta**; E15/E18 **medem e otimizam**.
