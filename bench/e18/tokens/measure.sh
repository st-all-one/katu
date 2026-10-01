#!/usr/bin/env bash
# Q-01 — mede o rácio bytes/token do modelo local (llama.cpp `/tokenize`).
#
# Corpus **fixo** (abaixo), pelo que o número é reproduzível byte-a-byte. Não toca no repositório
# nem no log do katu. Uso: `bench/e18/tokens/measure.sh` (servidor em `$KATU_TOKENIZE_URL`).
set -euo pipefail

URL="${KATU_TOKENIZE_URL:-http://127.0.0.1:8080/tokenize}"
MODEL="${KATU_TOKENIZE_MODEL:-qwen2.5-coder-1.5b}"

tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT

# Corpus representativo do prompt do katu: prosa PT (prime/AGENTS), descrições de tools em
# inglês, código Rust, JSON Schema/TOON e caminhos. Fixo ⇒ o número é reproduzível.
cat >"$tmp" <<'CORPUS'
katu prime v3
tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory
saida: TOON colunar v3 (D39) SEM headers; o esquema vive aqui:
- tabela: `\x1eNOME` e depois linhas com celulas separadas por `\x1f`, na ordem indicada;
- literal: `\x1dNOME` e depois linhas cruas ate a proxima secao;
- vazio = celula ausente; `{a,b}` = dominio fechado; booleano 0/1; sem floats.
O katu é um kernel agentic determinístico: cada pedido do modelo é logado antes de executar,
a política decide e só então o executor corre. Nada observável pelo modelo escapa ao log.
Use when you need file contents or structure. Do not use for searching many files (use grep).
Use when changing an existing file with a unique anchor. Do not use for new files (use write).
Use when running a program with known argv. Do not use for evaluating a shell string.
O caminho do ficheiro a editar; o trecho único a substituir tem de aparecer exatamente uma vez.
O orçamento de contexto é o mínimo para o cru e o teto para o resumido; a contagem de tokens é uma
estimativa determinística (bytes/token), não um tokenizer de provider.
use katu_core::diag::{Level, events};
fn tokens_from_bytes(bytes: usize) -> usize {
    let _span = crate::trace_fn!("context::tokens_from_bytes");
    bytes.div_ceil(4)
}
fn fit_raw(messages: &[Message], raw_min: usize) -> Vec<Message> {
    let mut used = 0usize;
    let mut start = messages.len();
    for (index, message) in messages.iter().enumerate().rev() {
        let cost = message_weight(message);
        if used.saturating_add(cost) > raw_min { break; }
        used = used.saturating_add(cost);
        start = index;
    }
    messages.get(start..).unwrap_or_default().to_vec()
}
{"type":"object","properties":{"path":{"type":"string","description":"caminho do ficheiro a ler"},"view":{"type":"string","enum":["full","range","outline","summary","symbol","diff"],"description":"forma do resultado"}},"required":["path"]}
crates/katu-core/src/context.rs
crates/katu/src/agent/turn/request.rs
target/debug/deps/libkatu_core-3f9a1c0e2b7d4a5f.rlib
batch_artifacts=6
diag_events=113
xtask_commands=29
CORPUS

bytes="$(wc -c <"$tmp")"
tokens="$(curl -s -m 30 -X POST "$URL" -H 'content-type: application/json' \
  --data-binary "$(python3 - "$tmp" <<'PY'
import json, sys
print(json.dumps({"content": open(sys.argv[1], encoding="utf-8").read()}))
PY
)" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["tokens"]))')"

python3 - "$bytes" "$tokens" "$MODEL" <<'PY'
import json, sys
bytes_, tokens, model = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
ratio = bytes_ / tokens
print(json.dumps({
    "schema": "katu.bench.tokens.v1",
    "model": model,
    "corpus": "bench/e18/tokens/measure.sh (heredoc CORPUS, fixo)",
    "bytes": bytes_,
    "tokens": tokens,
    "bytes_per_token": round(ratio, 3),
    "baseline_bytes_per_token": 4.0,
    "deviation_pct": round((ratio - 4.0) / 4.0 * 100, 2),
}, sort_keys=True))
PY
