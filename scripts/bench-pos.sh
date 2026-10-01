#!/usr/bin/env bash
# bench-pos — leituras pós-optimização nos mesmos eixos do baseline E18 (`bench/e18/`).
#
# Porquê existe: o `bench/e18/raw.json` é o retrato **antes** de optimizar e continua publicado
# como se fosse o estado actual. Este script produz a leitura de **hoje** (turno e2e, log.append,
# fs.write, session.open, memory.open, prompt, cache) para que a comparação seja explícita — e
# para que fique claro o que é **atribuível** (contagens) e o que **não é** (latências entre
# máquinas diferentes).
#
# Uso:
#   scripts/bench-pos.sh [--base URL] [--model PATH] [--warm N] [--cold N] [--out PATH]
#
# Requer um `llama serve` a responder em `--base` (por omissão http://127.0.0.1:8081/v1) e um
# binário com instrumentação: `cargo build -p katu --release --features profile`.
#
# O artefacto é **de medição**, não um gate: nenhum `cargo test` o invoca. As leituras que
# sustentam decisões entram em `bench/published.toml` com base tipada (DF5).

set -euo pipefail

BASE="http://127.0.0.1:8081/v1"
MODEL="${HOME}/.local/share/katu/models/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"
WARM=9
COLD=4
OUT="bench/e18/pos/raw.json"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/katu"

while [ $# -gt 0 ]; do
    case "$1" in
        --base) BASE="$2"; shift 2 ;;
        --model) MODEL="$2"; shift 2 ;;
        --warm) WARM="$2"; shift 2 ;;
        --cold) COLD="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        *) printf 'opção desconhecida: %s\n' "$1" >&2; exit 2 ;;
    esac
done

command -v curl >/dev/null || { echo "curl ausente" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 ausente" >&2; exit 2; }
[ -x "${BIN}" ] || { echo "compile antes: cargo build -p katu --release --features profile" >&2; exit 2; }
# A instrumentação é uma **feature**: um `cargo build` posterior sem `--features profile` reescreve
# o binário e a recolha sai com zero spans. Falhar aqui, em vez de publicar zeros.
spans_present=$(strings "${BIN}" 2>/dev/null | grep -c "katu\.fn" || true)
if [ "${spans_present}" = "0" ]; then
    echo "binário sem instrumentação: rebuild com --features profile" >&2
    exit 2
fi
curl -s -m 5 "${BASE}/models" >/dev/null || { echo "provider não responde em ${BASE}" >&2; exit 2; }

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/katu-pos-XXXXXX")"
LOGS="${SCRATCH}/logs"
mkdir -p "${LOGS}"
trap 'rm -rf "${SCRATCH}"' EXIT

# Projecto de scratch: mesma configuração do baseline, directório isolado (nada toca o repo).
(
    cd "${SCRATCH}"
    "${BIN}" config set provider llama >/dev/null
    "${BIN}" config set base "${BASE}" >/dev/null
    "${BIN}" config set model "${MODEL}" >/dev/null
    "${BIN}" config set embeddings.url "${EMBEDDINGS:-http://127.0.0.1:8889/v1}" >/dev/null
)

turn() { # <rótulo> <fase>
    local label="$1"
    local phase="$2" log="${LOGS}/${label}.log" out
    out=$(cd "${SCRATCH}" && KATU_INSTRUMENT=1 timeout 180 "${BIN}" run "responda com uma palavra" \
        --log-level trace --json 2>"${log}")
    printf '%s\n' "${out}" >"${LOGS}/${label}.json"
    if [ ! -s "${log}" ]; then
        echo "turno ${label}: sem traço de instrumentação (stderr vazio)" >&2
        exit 3
    fi
}

for i in $(seq 1 "${COLD}"); do turn "first${i}" first; done
for i in $(seq 1 "${WARM}"); do turn "warm${i}" warm; done

KATU_POS_LOGS="${LOGS}" KATU_POS_OUT="${ROOT}/${OUT}" KATU_POS_BASE="${BASE}" \
KATU_POS_MODEL="${MODEL}" KATU_POS_ROOT="${ROOT}" python3 - <<'PY'
import json, os, platform, re, subprocess, sys

logs = os.environ["KATU_POS_LOGS"]
root = os.environ["KATU_POS_ROOT"]

SPAN = re.compile(r"kind=span\.end function=(\S+) event=(\S+) dur_ns=Some\((\d+)\)")


def read_rows():
    """Um por traço de turno, em ordem de ficheiro (cold primeiro, depois warm)."""
    rows = []
    for name in sorted(os.listdir(logs)):
        if not name.endswith(".log"):
            continue
        spans = SPAN.findall(open(os.path.join(logs, name), encoding="utf-8", errors="replace").read())
        if not spans:
            sys.exit(f"{name}: zero spans — o binário foi reconstruído sem `--features profile`?")
        by_event = {}
        for function, event, ns in spans:
            by_event.setdefault(event, []).append(int(ns))
        report = json.load(open(os.path.join(logs, name.replace(".log", ".json")), encoding="utf-8"))
        usage = report.get("data", {}).get("usage", {})

        def total(event):
            return sum(by_event.get(event, []))

        def first(event):
            values = by_event.get(event)
            return values[0] if values else 0

        def outermost(event):
            """Maior span do evento: os spans são aninhados (`katu.fn` dentro de `provider.request`),
            e o externo é o que representa a operação."""
            values = by_event.get(event)
            return max(values) if values else 0

        rows.append({
            "phase": "first" if name.startswith("first") else "warm",
            "run_ms": round(first("katu.run") / 1e6, 3),
            "provider_ms": round(outermost("provider.request") / 1e6, 3),
            "log_append_sum_ms": round(total("log.append") / 1e6, 3),
            "log_append_max_ms": round(max(by_event.get("log.append", [0])) / 1e6, 3),
            "log_append_events": len(by_event.get("log.append", [])),
            "fs_write_sum_ms": round(total("fs.write") / 1e6, 3),
            "fs_write_events": len(by_event.get("fs.write", [])),
            "session_open_ms": round(outermost("session.open") / 1e6, 3),
            "memory_open_ms": round(outermost("memory.open") / 1e6, 3),
            "prompt_tokens": usage.get("input", 0),
            "cached_tokens": usage.get("cached", 0),
        })
    return rows


def median(values):
    ordered = sorted(values)
    if not ordered:
        return 0.0
    middle = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[middle]
    return (ordered[middle - 1] + ordered[middle]) / 2


rows = read_rows()
warm = [row for row in rows if row["phase"].startswith("warm")] or rows
first_turns = [row for row in rows if row["phase"] == "first"]
summary = {
    key: round(median([row[key] for row in warm]), 3)
    for key in (
        "run_ms", "provider_ms", "log_append_sum_ms", "log_append_max_ms", "fs_write_sum_ms",
        "session_open_ms", "memory_open_ms", "prompt_tokens", "cached_tokens",
    )
}
summary["log_append_events"] = warm[0]["log_append_events"]
summary["fs_write_events"] = warm[0]["fs_write_events"]
summary["non_provider_ms"] = round(summary["run_ms"] - summary["provider_ms"], 3)
summary["non_provider_share_milli"] = int(
    round(1000 * (summary["run_ms"] - summary["provider_ms"]) / max(summary["run_ms"], 1e-9))
)
summary["cache_hit_ratio"] = round(
    summary["cached_tokens"] / max(summary["prompt_tokens"], 1), 4
)
if first_turns:
    # «primeiro turno» não é «arranque a frio»: só é cold start se o servidor de inferência não
    # tiver o prefixo em cache — dai `first_turn_cached_tokens`. Para um cold start real é preciso
    # reiniciar o servidor. Declarado, não assumido.
    summary["first_turn_ms"] = first_turns[0]["run_ms"]
    summary["first_turn_cached_tokens"] = first_turns[0]["cached_tokens"]

cpu = ""
with open("/proc/cpuinfo", encoding="utf-8") as handle:
    for line in handle:
        if line.startswith("model name"):
            cpu = line.split(":", 1)[1].strip()
            break

value = {
    "schema": "katu.bench.pos.v1",
    "question": "quais leituras mudaram desde o baseline E18 (antes de optimizar)",
    "baseline_artifact": "bench/e18/raw.json",
    "environment": {
        "cpu": cpu,
        "threads": os.cpu_count(),
        "kernel": platform.release(),
        "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip(),
        "base": os.environ["KATU_POS_BASE"],
        "model": os.environ["KATU_POS_MODEL"],
        "note": "a máquina do baseline E18 é outra (AMD Ryzen 5 5500U, 12 threads); ver PROTOCOL.md",
    },
    "summary": summary,
    "turns": rows,
    "attribution": {
        "comparable_across_machines": ["prompt_tokens", "cached_tokens", "log_append_events", "fs_write_events"],
        "not_comparable_across_machines": [
            "run_ms", "wall_ms", "provider_ms", "log_append_sum_ms", "fs_write_sum_ms",
            "session_open_ms", "memory_open_ms",
        ],
        "why": "tokens e contagens de eventos não dependem do hardware; os tempos dependem da CPU, "
               "do armazenamento e da máquina do servidor de inferência — por isso não se publicam",
    },
}

with open(os.path.join(root, os.environ["KATU_POS_OUT"].replace(root + "/", "")), "w", encoding="utf-8") as handle:
    json.dump(value, handle, indent=2)
    handle.write("\n")
print(f"artefacto: {os.environ['KATU_POS_OUT']}")
print(json.dumps(summary, indent=1))
PY