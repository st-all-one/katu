# 11 — Embeddings, fila e worker

Embeddings são **derivados e opcionais** (D42/D79). O sistema **nunca bloqueia**
por embedding: notas recém-criadas ficam *dark* até a fila derivada drená-las, e
o provedor é plugável via config.

## 1. Provedor plugável (D79/D101/D202)

`embeddings.provider` ∈ `http` (default) | `lightweight` | `none`:

| Provider | O que é |
|---|---|
| `http` | servidor local **OpenAI-compatible** (`llama-server … --embeddings`, TEI/Ollama/vLLM) em `embeddings.endpoint` (default `http://127.0.0.1:8889/v1/embeddings`) |
| `lightweight` | embedder determinístico por **hash** — CI/offline, sem rede |
| `none` | cai para BM25 puro |

- Cliente **HTTP/1.1 bloqueante sobre `std::net`** (sem `tokio`/`reqwest`);
  `https://` exige proxy/TLS terminator. **Sem inferência in-process.**
- Modelo default: `ibm-granite/granite-embedding-97m-multilingual-r2` (384d,
  D123). `revision` pinada; o índice é **invalidado** quando modelo/revisão/
  dimensão mudam.

## 2. Identidade e álgebra

- `EmbeddingMeta { provider, model, revision, dimensions, similarity }` +
  `Similarity` (cosseno/dot).
- `vector.rs`: `cosine`, `dot`, `normalize`, `is_normalized`, `l2_norm`,
  `similarity`.

## 3. Índice (D80/D84)

`.idx/embeddings.jsonl` com `IndexedVector` (id + vetor + `meta`). Toda remoção
de nota descarta o vetor (`purge_derived`); o `doctor` detecta divergência
canônico↔derivado.

## 4. Cache (D83/D148/D153)

- Chave lógica **`(body_hash, modelo)`**, com teto e eviction **LRU**.
- **Versionável** (opt-in `embeddings.version_cache`, D148): mora em
  `.knudge/emb_cache.jsonl` (fora do `.idx/`), com `merge=union` + dedup e
  **sem eviction** quando versionado.
- Loader idempotente, **model-aware** e determinístico (`created_ms`).
- Falha de embedding marca `pending`, **nunca descarta a nota**.

## 5. Estado e modo (D80/D131)

- `EmbeddingState` ∈ `Indexed`/`Pending`/`Stale` (derivado).
- `EmbeddingMode` ∈ `lazy` (default) | `manual`. `eager` é rejeitado (exit 7).
- Em `lazy`, ao fim de cada invocação não-`maintenance` o CLI drena **um lote**
  best-effort (`idle::maybe_drain`), **depois** de emitir a saída — nunca altera
  exit code nem `warnings[]`.

## 6. Dreno e reconciliação (D80/D85/D182)

- `pipeline::drain` consome a fila e reconcilia: cache → provedor → índice.
- `flush.rs`: flush **coalescido** (debounce, `flush_ms`) do `.idx`/embeddings
  com *dirty flag*, e flush forçado na saída — rajadas de 10–20 notas causam um
  único rewrite.

## 7. Consultas semânticas (D102/D158)

- `rank_query` (cosseno/dot, brute-force) alimenta o canal vetorial do `ask`.
- `suggest` classifica pares em `duplicate`/`link`/`contradiction` (§`05`).
  **Advisory**: persiste em `.idx/suggestions.jsonl`, nunca vira aresta.
  Determinístico, zero-LLM.

## 8. `kd drain`

```
kd drain [--status | --digest [--force]]
kd drain service [--install|--status|--subscribe|--unsubscribe|--reconcile|--uninstall]
```

```bash
kd drain --status          # provedor, modo, dimensões, pendentes, desatualizados
kd drain --digest          # indexa agora (repita para mais)
kd drain --digest --force  # apaga o índice derivado e refaz do zero (último recurso)
```

`--force` é último recurso (reindexação demorada). Se o provedor cair no meio, o
knudge **não trava a fila**: mantém as notas pendentes e emite **um** aviso.

## 9. Instalação do worker + servidor

### Rápida (recomendada)

```bash
kd drain service --install
```

Faz pré-flight (`kd`/`llama`/GGUF/projeto), baixa `llama.cpp` + GGUF se faltarem
(revisão pinada + SHA-256), sobe o servidor de embeddings persistente
(`knudge-embed`) em `127.0.0.1:8889` via `systemd --user` (Linux) ou `launchd`
(macOS), e cadastra o projeto para drain periódico.

```bash
kd drain service --status        # saúde + probe do endpoint
kd drain service --subscribe     # cadastra OUTRO projeto (multi-projeto)
kd drain service --unsubscribe   # descadastra este (mantém o sistema)
kd drain service --reconcile     # alinha endpoint/model ao worker e reindexa
kd drain service --uninstall     # remove agendador + servidor (preserva o GGUF)
kd drain service --install --dry-run   # mostra URL/revisão/hash do modelo
```

Flags: `--every <30m|1h|1d>` (default `1h`), `--port <N>` (default `8889`),
`--model <PATH>`, `--no-deps`, `--keep-model`/`--remove-model`,
`--script <PATH>` | `--url <URL> --sha256 <HEX>`.

- Sem `systemd`/`launchd`, o `--install` **recusa** e imprime a linha de `cron`
  equivalente.
- O wrapper é cross-platform; o script embutido é Unix. No Windows, use
  `--script worker.ps1`.
- **Supply-chain (D183):** GGUF de revisão pinada + SHA-256; instalador do
  llama.cpp verificado antes de executar — nunca `curl … | sh` cego.

### Manual (sem worker)

```bash
# 1) llama.cpp
brew install llama.cpp        # ou apt/dnf/winget/scoop

# 2) modelo GGUF (granite 97m-r2, 384d)
mkdir -p ~/.config/local/knudge
wget -O ~/.config/local/knudge/granite-97m-r2-Q8_0.gguf \
  https://huggingface.co/mykor/granite-embedding-97m-multilingual-r2-GGUF/resolve/<REV>/granite-embedding-97M-multilingual-r2-Q8_0.gguf
sha256sum ~/.config/local/knudge/granite-97m-r2-Q8_0.gguf

# 3) servidor + apontar o projeto
llama serve -m ~/.config/local/knudge/granite-97m-r2-Q8_0.gguf \
  --embeddings --pooling mean -b 2048 -ub 2048 --host 127.0.0.1 --port 8889

kd config set --key embeddings.provider --value http
kd config set --key embeddings.model --value ibm-granite/granite-embedding-97m-multilingual-r2
kd config set --key embeddings.dimensions --value 384
kd config set --key embeddings.endpoint --value http://127.0.0.1:8889/v1/embeddings
kd drain --digest
```

> ⚠️ **Sempre use `-ub 2048`.** O default `512` rejeita notas longas e o drain
> falha com `indexed=0`. `-b 2048 -ub 2048` cobre o maior corpo de nota.

Desligar o canal: `kd config set --key recall.semantic --value false`.

## 10. Cache versionado e trabalho em equipe

O vetor é função pura de `(modelo, conteúdo)`, então o cache pode ser versionado
para que um clone reindexe **sem chamar o modelo**:

```bash
kd config set --key embeddings.version_cache --value true
kd sync --message "notas + cache"
# num clone novo:
git pull && kd drain --digest
```

Premissa: **todos usam o mesmo modelo e as mesmas configurações**.

## 11. Suporte por SO

| SO | Shell | Worker |
|---|---|---|
| Linux | `bash` | `kd drain service --install` (systemd `--user`) |
| macOS | `bash` | `kd drain service --install` (launchd) |
| Windows | PowerShell | embutido é Unix → `--script worker.ps1` ou manual |

## 12. Configuração de embeddings

| Chave | Default | Para quê |
|---|---|---|
| `embeddings.provider` | `http` | `http`/`lightweight`/`none` |
| `embeddings.model` | granite | identidade do modelo |
| `embeddings.dimensions` | `384` | dimensão do vetor |
| `embeddings.similarity` | `cosine` | métrica |
| `embeddings.mode` | `lazy` | `lazy`/`manual` |
| `embeddings.batch` / `max_pending` | `32` / `1000` | lote e backpressure |
| `embeddings.cache` / `version_cache` | `true` / `false` | cache |
| `embeddings.endpoint` / `timeout_ms` / `retries` | `:8889` / 30000 / `2` | servidor HTTP |
| `recall.semantic` / `semantic_weight` | `true` / `30.0` | canal semântico |

## 13. Troubleshooting

| Sintoma | Causa | Ação |
|---|---|---|
| `indexed=0` no drain | `-ub` pequeno (512) | suba o servidor com `-ub 2048` |
| "provedor inalcançável" | servidor fora do ar | `kd drain service --status` |
| Notas longas presas em `pending` | `-ub` pequeno / nota grande | reinicie com `-b 2048 -ub 2048` |
| `Connection refused` no `--digest` | nada em `:8889` | `kd drain service --install` ou manual |
| Worker não instala | sem systemd/launchd | use a linha de `cron` ou o modo manual |
| Sem resultados semânticos | canal desligado | `kd config set --key recall.semantic --value true` |
