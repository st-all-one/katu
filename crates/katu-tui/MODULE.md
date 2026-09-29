# `katu-tui`

**Épico:** E10 · **Fase:** 6.

A **interface de terminal** do katu (uma das duas superfícies, com a CLI — G7).

## Responsabilidade

- UX de codificação sobre `ratatui` 0.30 + `crossterm` 0.29 (estado central, render puro).
- Panic-safe: restaura o terminal em qualquer saída.
- Controlos do core: modelo/grau de pensamento, compactar, ver lixeira.

## Fronteira

- Sem servidor, sem rede (G7). Não é um protocolo.
