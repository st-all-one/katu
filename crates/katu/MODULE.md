# `katu` (binário)

**Épico:** E01/E04 · **Fase:** 0/2.

O **binário**: CLI, composição das portas e adaptador in-process do `knudge`. É aqui que vive
todo o código impuro confinado.

## Responsabilidade

- CLI (`clap`) e wiring das portas (`Clock`, `Rng`, `Fs`, `Env`, `Logger`).
- Adaptadores das portas em `src/ports/` (`mod.rs` = relógio/RNG/env/processo; `fs/` = `StdFs` com
  escrita atómica **endurecida**: temporário exclusivo `O_EXCL`/`0600` e nome imprevisível, para não
  seguir um symlink plantado — E07-T04).
- Adaptador in-process da porta `Memory` sobre o `knudge-core` (`KnudgeBuilder`, D214) — o único
  sítio com dependência do knudge.
- Exit codes na borda (a lógica propaga `Result`).
- Harness de medição do MVK (`examples/measure_mvk.rs`, feature `profile`, E05-T06): corre o
  caminho real e grava `bench/mvk/raw.json` (evidência tipada, DF5).

## Fronteira

- Pode depender de todos os crates, mas mantém o **firewall** a montante: os crates puros não
  dependem dele.
