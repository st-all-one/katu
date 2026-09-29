#!/usr/bin/env bash
# Gate de tamanho de arquivo (D92): arquivos de produção em crates/*/src e xtask/src ≤ 300 linhas.
# Módulos de teste embutidos (#[cfg(test)]) contam; por isso o limite é aplicado ao arquivo.
set -euo pipefail

limit=300
status=0

while IFS= read -r -d '' file; do
    lines=$(wc -l <"$file")
    if ((lines > limit)); then
        printf 'ERRO: %s tem %d linhas (limite %d)\n' "$file" "$lines" "$limit" >&2
        status=1
    fi
done < <(find crates xtask \
    -path 'crates/knudge' -prune -o \
    -type f -name '*.rs' -path '*/src/*' -print0)
# `crates/knudge` é um submódulo (workspace aninhado) com o seu próprio gate; fica em `crates/`
# apenas por organização e resolve-se sozinho — não é medido aqui.

if ((status == 0)); then
    printf 'file-length: ok (limite %d)\n' "$limit"
fi

exit "$status"
