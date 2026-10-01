# Memória e `unsafe`

Política de memória e código inseguro, decidida em ADR 0016.

## Regras de memória

- `#![forbid(unsafe_code)]` em todos os crates puros.
- `unsafe` só no sandbox/FFI, com `// SAFETY:` e justificação.
- Tipos proibidos: `Rc`, `Weak`, `RefCell`, `Cell`, `LinkedList`, `HashMap`, `HashSet`.
- Pânico, indexação e conversões `as` negados.

## Lar da decisão

[`../adr/0016-politica-de-memoria-e-unsafe.md`](../../adr/0016-politica-de-memoria-e-unsafe.md).
