#!/usr/bin/env bash
# Modelo local recomendado do katu (llama.cpp): Qwen2.5-Coder-1.5B-Instruct Q4_K_M.
#
# Uso:
#   scripts/llama.sh fetch [TAG]     # baixa o binário do llama.cpp + o GGUF Q4_K_M
#   scripts/llama.sh serve [PORTA]   # sobe o llama-server (default 8080)
#   scripts/llama.sh chat  [PORTA]   # smoke-test via /v1/chat/completions
#
# Destino: ~/.local/share/katu/{llama.cpp,models} (fora do repositório).
# Ativação no katu:
#   katu config set base http://127.0.0.1:8080/v1   # global (ou no projeto)
#   katu run "olá" --provider llama                  # usa o servidor local
#
# Referências: https://github.com/ggml-org/llama.cpp/releases (binário pré-compilado)
#              https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF
set -euo pipefail

BASE="${HOME}/.local/share/katu"
MODEL_REPO="Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF"
MODEL_FILE="qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"
DEFAULT_TAG="b11292"

fetch() {
  local tag="${1:-$DEFAULT_TAG}"
  mkdir -p "${BASE}/llama.cpp" "${BASE}/models"
  local url="https://github.com/ggml-org/llama.cpp/releases/download/${tag}/llama-${tag}-bin-ubuntu-x64.tar.gz"
  echo "== llama.cpp ${tag} =="
  curl -L --fail -o /tmp/llama-bin.tar.gz "${url}"
  tar -xzf /tmp/llama-bin.tar.gz -C "${BASE}/llama.cpp" --strip-components=1
  echo "== modelo ${MODEL_FILE} =="
  if command -v hf >/dev/null 2>&1; then
    hf download "${MODEL_REPO}" "${MODEL_FILE}" --local-dir "${BASE}/models"
  else
    curl -L --fail -o "${BASE}/models/${MODEL_FILE}" \
      "https://huggingface.co/${MODEL_REPO}/resolve/main/${MODEL_FILE}"
  fi
  echo "pronto: ${BASE}/llama.cpp/llama-server + ${BASE}/models/${MODEL_FILE}"
}

serve() {
  local port="${1:-8080}"
  export LD_LIBRARY_PATH="${BASE}/llama.cpp${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
  exec "${BASE}/llama.cpp/llama-server" \
    -m "${BASE}/models/${MODEL_FILE}" \
    --host 127.0.0.1 --port "${port}" -c 4096 -t 4
}

chat() {
  local port="${1:-8080}"
  curl -s "http://127.0.0.1:${port}/v1/chat/completions" \
    -H 'content-type: application/json' \
    -d '{"model":"local","messages":[{"role":"user","content":"diga apenas: oi"}],"max_tokens":16}'
  echo
}

case "${1:-}" in
  fetch) fetch "${2:-}" ;;
  serve) serve "${2:-}" ;;
  chat) chat "${2:-}" ;;
  *) echo "uso: $0 {fetch [TAG] | serve [PORTA] | chat [PORTA]}" >&2; exit 2 ;;
esac
