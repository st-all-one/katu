# `katu` (binário)

**Épico:** E01/E04 · **Fase:** 0/2.

O **binário**: CLI, composição das portas e adaptador in-process do `knudge`. É aqui que vive
todo o código impuro confinado.

## Responsabilidade

- CLI (`clap`) e wiring das portas (`Clock`, `Rng`, `Fs`, `Env`, `Logger`).
- Adaptador in-process da porta `Memory` sobre o `knudge-core` (`KnudgeBuilder`, D214) — o único
  sítio com dependência do knudge.
- Exit codes na borda (a lógica propaga `Result`).

## Fronteira

- Pode depender de todos os crates, mas mantém o **firewall** a montante: os crates puros não
  dependem dele.
