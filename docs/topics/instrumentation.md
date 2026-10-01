# Instrumentação

Um facto, **um id**. Todo o evento/span vem do catálogo único
(`crates/katu-core/src/diag/events.rs`), verificado por `cargo xtask check-diag`.

## Regras de instrumentação

- Formato `<subsistema>.<ação>`, minúsculas.
- Estável: mudar um id é decisão registada.
- Instrumentar **não** é observável pelo modelo nem pelo log de sessão.
- O teto de superfície (`surface.toml`) é verificado por `cargo xtask check-surface`.
- **Cobertura por função:** `cargo xtask diag:coverage` exige **≥ 90 %** em **duas** medidas —
  funções instrumentáveis e todas as funções (com as `const fn` no denominador). Código de
  produção apenas: `tests/`, `examples/` e `benches/` ficam fora, tal como o próprio `diag`
  (recursão no sink) e a `katu-policy` (firewall: é instrumentada pelo chamador).
- Uma `const fn` **não** pode abrir um span num build com `feature = "instrument"` (o
  `Span::start_function` não é `const`): para a contar é preciso tirar-lhe o `const` (só quando não
  é usada em contexto `const`) — ou não a ter.
- **Atribuição por função:** o `trace_fn!` usa o identificador genérico `katu.fn` **de propósito**
  (S-02); o rótulo real (`módulo::função`) viaja em `Record.function` e o sink de `stderr`
  imprime-o como `function=…` (Q-09, nível `trace`). Assim o id genérico não é ambíguo.

## Leitura

[`../../plan/20-instrumentacao-transversal.md`](../../plan/20-instrumentacao-transversal.md).
