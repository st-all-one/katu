#!/bin/sh
# Worker de auto-drain do katu (E20-T20).
#
# Materializado por `katu memo drain --watch-service --install` e agendado por um timer systemd
# `--user`. Corre `katu memo drain --digest` em cada projeto subscrito (um caminho por linha em
# `$XDG_DATA_HOME/katu/watched`). Silencioso e idempotente: nunca falha o timer.
set -eu
base="${XDG_DATA_HOME:-$HOME/.local/share}/katu"
watched="$base/watched"
katu="${KATU_BIN:-katu}"
[ -f "$watched" ] || exit 0
while IFS= read -r project; do
  [ -n "$project" ] || continue
  [ -d "$project" ] || continue
  (cd "$project" && "$katu" memo drain --digest >/dev/null 2>&1) || true
done < "$watched"
