# Instrumentação

Um facto, **um id**. Todo o evento/span vem do catálogo único
(`crates/katu-core/src/diag/events.rs`), verificado por `cargo xtask check-diag`.

## Regras de instrumentação

- Formato `<subsistema>.<ação>`, minúsculas.
- Estável: mudar um id é decisão registada.
- Instrumentar **não** é observável pelo modelo nem pelo log de sessão.
- O teto de superfície (`surface.toml`) é verificado por `cargo xtask check-surface`.

## Leitura

[`../../plan/20-instrumentacao-transversal.md`](../../plan/20-instrumentacao-transversal.md).
