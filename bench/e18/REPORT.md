# Relatório — baseline de arranque do E18

> Retrato do custo do projeto **antes** de otimizar (E18 §0.3). Artefacto cru:
> [`raw.json`](raw.json). Protocolo: [`PROTOCOL.md`](PROTOCOL.md). Números publicados:
> `bench/published.toml` (base `measured`).

## 1. Servidores (estado durável)

| Papel | Unidade | Endpoint | Modelo | Estado |
|---|---|---|---|---|
| Geral (chat) | `katu-llama.service` | `127.0.0.1:8080/v1` | `qwen2.5-coder-1.5b-instruct-q4_k_m` | `enabled`+`active` |
| Embeddings | `knudge-embed.service` | `127.0.0.1:8889/v1` | `granite-embedding-97m-multilingual-r2` (384d) | `enabled`+`active` |
| Remoto | — | `opencode.ai/zen/go/v1` | `longcat-2.5-preview-free` | chave efémera `KATU_OPENCODE_KEY` |

Ambos os locais correm sob systemd `--user` (arranque automático, `Restart=on-failure`). A config
global do katu (`~/.config/local/katu/katu.toml`) aponta `provider=llama`, `base=…:8080/v1` e a
segunda IA em `embeddings.url=…:8889/v1`; o knudge foi alinhado para o mesmo endpoint (havia
*drift*: config dizia `:8080`, o worker corria em `:8889`).

## 2. Baseline (números)

| Métrica | p50 | p95 | Artefacto |
|---|---|---|---|
| Arranque `--version` | **6,47 ms** | 7,43 ms | `bench/e18/raw.json` |
| Arranque `prime` | 6,09 ms | 7,50 ms | idem |
| Embeddings (1 nota, granite 97m) | **16,10 ms** | 17,45 ms | idem |
| Provider `llama` TTFT (quente) | **68 ms** | 523 ms | idem |
| Provider `llama` total | 720 ms | 1303 ms | idem |
| Provider `opencode-go` TTFT | **2 304 ms** | 4 086 ms | idem |
| Provider `opencode-go` total | 3 525 ms | 8 043 ms | idem |
| Turno e2e `llama` (quente) | **360 ms** | 562 ms | idem |
| Turno e2e `opencode-go` | 343 ms | 381 ms | idem |
| **Overhead fora do provider** | **72,3 ms** | — | idem |
| ↳ `log.append` (5×/turno) | 18,3 ms | — | idem |
| ↳ `fs.write` (7×/turno) | 25,8 ms | — | idem |
| ↳ `session.open` | 8,5 ms | — | idem |

**Contexto do turno:** `input_tokens = 3265`, com `cached = 3254` no 2.º turno (prefix-cache do
servidor). O **cold start** do modelo local (primeiro pedido, `cached=0`) leva ~40 s: 3265 tokens de
prompt a ~51 tok/s (CPU) na fase de *prompt eval*. Com cache quente, o mesmo turno cai para
**≈0,95 s** (llama) e **≈0,51 s** (opencode-go).

## 3. Pontos de lentidão (por ordem de impacto)

1. **Custo do provider domina o turno** (E18-F4). Fora dele, o projeto gasta ~72 ms/turno. O
   gargalo real no local é o **tamanho do prompt** (3265 tokens) e a ausência de cache no 1.º
   pedido. A 2.ª IA (embeddings) custa 16 ms/nota — um dreno de N notas é N×16 ms.
2. **Persistência do log ≈ 18 ms/turno** (F5). `Log::append` faz `sync_data()` (fsync) por evento
   (`crates/katu/src/ports/fs/mod.rs`); 5 eventos/turno = 5 fsync. Escala com o número de eventos —
   num turno agêntico com tools, multiplica. Candidato a *group commit* opt-in.
3. **Snapshots/escritas atómicas ≈ 26 ms/turno** (F5). 7 `fs.write` (tmp→`sync_all`→`rename`) por
   turno de 1 passo. Mesma natureza: durabilidade por evento.
4. **`session.open` ≈ 8,5 ms** (F5) — inclui `KnudgeMemory::open` + `identity::create` + leitura de
   log/snapshot. **Não instrumentado por dentro** (ver §4), pelo que não se sabe quanto é índice do
   knudge vs I/O.
5. **Arranque < 10 ms** — dentro da meta de 50 ms (`plan/16`). Não é gargalo.
6. **TOON/model-facing sem duração** (F2). `toon.project`/`toon.emit`/`model.project` são `event!`
   (sem `dur_ns`); o custo de montar o que vai ao modelo é invisível no agregado.

## 4. Auditoria de instrumentação — pontos cegos

### 4.1 Cobertura do catálogo

- Catálogo: **86 ids** (`CATALOG_VERSION=1`); **75 emitidos** em produção; **11 declarados mas
  nunca emitidos**:

  `audit.index`, `contain.check`, `contain.deny`, `context.trim`, `katu.shutdown`, `kernel.stop`,
  `memory.compact`, `memory.read`, `policy.waiver`, `store.load`, `store.save`.

  Nenhum id é emitido fora do catálogo (o gate `check-diag` segura isso). Os 11 órfãos são
  **pontos cegos de código**: ou a capacidade não existe, ou existe e não é medida.

### 4.2 Caminhos relevantes **sem span nenhum** (não estão no catálogo)

| Caminho | Ficheiro | Porque importa |
|---|---|---|
| `KnudgeMemory::open` | `crates/katu/src/memory/mod.rs` | 8,5 ms de `session.open` sem atribuição |
| `skills::discover` + `read_instructions` | `crates/katu-core/src/skill.rs` | scan de `.agents/skill*/*/SKILL.md` por arranque; alimenta o prompt |
| `load_rules` (`policy/memory.toml`) | `crates/katu/src/runtime.rs` | parse no arranque |
| `HttpEmbedder` (2.ª IA) | `crates/katu/src/memory/drain.rs` (`knudge-core`) | 16 ms/nota, invisível ao diag |
| `context.build` sem `tokens` | `crates/katu-core/src/context.rs` | mede-se a duração (11 µs), não o **tamanho** do prompt |
| `provider.request` sem `prompt_tokens` | `crates/katu-providers/src/{llama,opencode}.rs` | só há `provider`/`model`; o custo real (tokens) fica no `usage` pós-facto |

### 4.3 Drift de documentação

`SURFACE_IMPLEMENTATION.md` §7 promete eventos `cli.prime`, `memo.drain`, `tui.slash`, `mouse.copy`;
**nenhum existe** no catálogo. Ou nascem (código+catálogo+`surface.toml`) ou o doc corrige-se.

### 4.4 Boa cobertura (sem ação)

`fs.*` (7/7), `kernel.*` (6/7), `tool.*` (11/11), `provider.*` (7/7), `cost.*` (4/4), `session.*`
(3/3), `tui.*` (7/7), `verify.*`, `toon.*`. As portas quentes (kernel, política pelo chamador,
tools, provider) estão instrumentadas.

## 5. Próximos passos E18 (ancorados neste baseline)

1. **Fechar os pontos cegos** (§4.2) antes de otimizar: `memory.open`, `skills`, `embeddings`,
   `context.build{tokens}`, `provider.request{prompt_tokens}`. Sem eles, F2/F4/F5 otimizam às
   cegas — que é exatamente o que o E19 existe para evitar.
2. **F4 (transporte):** *hedging*/*backpressure* e buffer adaptativo (`plan/19` §4) — o baseline
   mostra TTFT 2,3 s/4,1 s (p50/p95) no remoto e 68 ms no local.
3. **F5 (estado/replay):** *group commit* do log e snapshots menos frequentes; medir com o
   `dhat`/`criterion` que o E18-T10 vai instalar. Hoje ~44 ms/turno em fsync+rename.
4. **F2/F3 (contexto):** atacar os 3265 tokens de prompt (catálogo de tools, skills, prime):
   submodular+MMR (F2) e compactação por entropia/JS (F3), com A/B contra este baseline.
5. **E18-T10:** formalizar este protocolo num harness (`criterion`/`hyperfine`/`dhat`, ≥3
   repetições, IC 95 %) e ligar ao `gate:bench`.

## 6. Reprodução

Os comandos estão em [`PROTOCOL.md`](PROTOCOL.md). O `raw.json` foi gerado a partir de:
`/tmp/e18-{startup,prime,embed}.txt`, `xtask provider-smoke`, 10×`katu run` por provider e 5×
`katu run --log-level trace` (agregado por `event`). Ao refazer, regenerar `raw.json` e revalidar
`cargo xtask gate:bench`.

## 7. Limites (honestidade)

- Números **desta máquina** (Ryzen 5 5500U, CPU, 4 threads); a 2.ª IA é CPU-local.
- Amostras pequenas (n=5–100); sem IC 95 % — isso é E18-T10.
- O cold start do modelo local (~40 s) é **declarado**, não escondido; o p50 reportado é a quente.
- A composição exata dos 3265 tokens (tools vs AGENTS.md vs prime vs skills) **não** foi medida
  linha a linha — é um dos pontos cegos a fechar (§4.2).
