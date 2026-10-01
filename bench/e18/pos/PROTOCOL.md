# Leituras pós-optimização — o que mudou desde o baseline E18

> Retrato **de hoje**, nos mesmos eixos do baseline [`bench/e18/`](../PROTOCOL.md) (que é o
> retrato **antes** de optimizar). Artefacto cru: [`raw.json`](raw.json). Gerador reprodutível:
> `scripts/bench-pos.sh`.

## Porquê este documento

O `bench/published.toml` publica os números `e18.*` — que são o **baseline**, não o estado actual.
Sem uma leitura de hoje, o ledger afirma que o projecto custa o que custava antes de P-01, P-04,
B-01 e da redução de prompt. Este documento preenche essa lacuna e, mais importante, **separa o
que é atribuível ao projecto do que é da máquina**.

## Método

- **Servidor**: `llama serve` com o **mesmo GGUF do baseline** (Qwen2.5-Coder-1.5B-Instruct
  Q4_K_M), `-c 4096 -t 4`, em `http://127.0.0.1:8081/v1` (a 8080 estava ocupada).
- **Binário**: `cargo build -p katu --release --features profile` (sem a feature, não há spans — o
  script falha em vez de publicar zeros).
- **Projecto**: directório de scratch com a mesma configuração do baseline; nada toca o repo.
- **Turnos**: 4 primeiros + 9 quentes, `katu run "responda com uma palavra"`, um passo, sem tools.
- **Medição**: spans de `KATU_INSTRUMENT=1 --log-level trace`; `provider.request` e `session.open`
  são lidos pelo span **externo** (os spans são aninhados; o primeiro não é a operação).

```sh
scripts/llama.sh serve 8081 &                     # ou: llama serve -m <gguf> --port 8081 -c 4096 -t 4
cargo build -p katu --release --features profile
scripts/bench-pos.sh --warm 9 --cold 4
```

## Ambiente (difere do baseline — leia antes de comparar)

| | baseline E18 | esta leitura |
|---|---|---|
| CPU | AMD Ryzen 5 5500U (12 threads) | **AMD Ryzen 7 7735HS (16 threads)** |
| armazenamento | não declarado | NVMe, btrfs em `/home` |

## Resultados

Medianas dos 9 turnos quentes:

| eixo | baseline | agora | leitura |
|---|---|---|---|
| turno (`katu.run`) | 360 ms | **78,6 ms** | ⚠️ não comparável (CPU) |
| `provider.request` | — | 74,1 ms | ⚠️ não comparável |
| **overhead não-provider** | 72,3 ms (**201 ‰** do turno) | **4,5 ms (57 ‰)** | ⚠️ absolutos não comparáveis; a **fracção** é o que se pode comparar |
| `log.append` (soma/turno) | 18,3 ms (5 eventos) | **0,137 ms (7 eventos)** | ⚠️.fsync por evento desapareceu (P-01) |
| `fs.write` (soma/turno) | 25,8 ms (7 escritas) | **0,134 ms (8 escritas)** | idem |
| `session.open` | 8,5 ms | **0,42 ms** | ⚠️ não comparável |
| **prompt** | **3265 tokens** | **2057 tokens** | ✅ **atribuível**: −37,0 % |
| cache do prefixo | 3254/3265 (99,7 %) | 2056/2057 (99,95 %) | ✅ comparável |
| eventos de log por turno | 5 | 7 | ✅ contagem |

## O que é atribuível ao projecto

1. **Prompt −37 %** (3265 → 2057 tokens). Contagens não dependem de hardware: é o ledger (W-9.2),
   o prime (Q-20), o catálogo de skills (Q-05) e o `AGENTS.md` (Q-19). Com o cache de prefixo a
   99,95 %, menos prompt é menos custo de atenção **e** menos tokens pagos.
2. **O fsync por evento desapareceu.** O baseline pagava 18,3 ms por `log.append`; hoje cada
   append é **~20–40 µs** e o turno faz **7** appends (mais eventos, 0,14 ms no total). É o
   *group commit* de P-01 confirmado *in situ*, não só no bench sintético.
3. **A fracção controlável do turno caiu de 201 ‰ para 57 ‰.** Com o provider a 94 % do turno, o
   projecto já não é o caminho crítico — que era precisamente o objectivo do E18-F4/F5.

## O que **não** é atribuível

Os absolutos de latência. O turno foi de 360 ms para 78,6 ms, mas isso é **sobretudo a CPU**: o
mesmo prompt de 2057 tokens faz prompt-eval a ~640 tok/s aqui contra ~51 tok/s no baseline. O
arranque a frio que medi separadamente (servidor reiniciado, `cached = 0`) deu **3 228 ms** contra
os ~40 s do baseline — também sobretudo hardware. **Nenhum destes números entra em
`published.toml` como ganho.**

## Decisão

- Publicam-se em `bench/published.toml` **apenas as leituras estruturais** (`prompt_tokens`,
  `cache_hit_ratio`, `eventos por turno`), com `basis = "measured"` e este artefacto.
- As latências ficam **fora** do ledger: entre máquinas não são comparáveis, e o ledger não tem
  campo para qualificar isso (ver [DF5](../../../crates/katu-core/MODULE.md)).
- **Falta** a medição end-to-end na máquina declarada do baseline (5500U). Só aí a comparação
  antes/depois em latência deixa de ser uma inferência.

## Limites

- Turno de **um passo, sem tool calls**: não exercita B-01 (medido à parte, 76 % reproduced), nem
  o custo de um turno agêntico.
- O modelo local não emite tool calls nativas, pelo que o caminho das tools fica por cobrir em e2e.
- `first_turn_*` é o primeiro turno **do processo**, não um arranque a frio: só é frio se o
  servidor de inferência não tiver o prefixo em cache (declara-se `first_turn_cached_tokens`
  para se ver qual dos dois é).
- A máquina está declarada no artefacto; comparar sem a ler é como nasceram os números falsos.