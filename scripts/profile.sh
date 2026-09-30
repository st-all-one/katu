#!/usr/bin/env bash
# Harness de perfil de amostragem (E18-T10): tempo atómico POR FUNÇÃO, sem instrumentar código.
#
# Uso: scripts/profile.sh [--freq N] -- <programa> [args...]
#   scripts/profile.sh -- ./target/release/katu run
#   scripts/profile.sh --freq 2000 -- ./target/release/katu tui
#
# Deteta `samply` e usa-o se existir; senão `perf`; senão imprime como instalar e sai 0 (não
# bloqueia o CI). Recompile com símbolos: o perfil `release` do katu faz `strip = "symbols"`.
set -euo pipefail

freq=999
while [[ $# -gt 0 ]]; do
    case "$1" in
        --freq)
            freq="$2"
            shift 2
            ;;
        --)
            shift
            break
            ;;
        *)
            break
            ;;
    esac
done

if [[ $# -eq 0 ]]; then
    echo "uso: scripts/profile.sh [--freq N] -- <programa> [args...]" >&2
    exit 2
fi

if command -v samply >/dev/null 2>&1; then
    exec samply record "$@"
fi

if command -v perf >/dev/null 2>&1; then
    out="${KATU_PERF_OUT:-/tmp/katu.perf}"
    perf record -F "$freq" -g --call-graph dwarf -o "$out" -- "$@"
    printf 'perf: artefacto em %s; reporte com: perf report -i %s\n' "$out" "$out" >&2
    exit 0
fi

cat >&2 <<'EOF'
Nenhum profiler instalado. Instale um:
  cargo install samply                    # sem root, UI no browser
  sudo apt install linux-perf             # ou: sudo pacman -S perf
Recompile com símbolos (o release do katu faz strip):
  CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none cargo build --release -p katu
EOF
exit 0
