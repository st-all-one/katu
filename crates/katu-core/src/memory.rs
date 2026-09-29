//! Porta `Memory` — tipos do katu (DF6).
//!
//! O contrato completo (`pre_write`, `pre_edit`, `session_end`, `status`) chega em E03. A
//! implementação in-process vive no binário (`katu/src/memory/`), isolada por `xtask check-layers`;
//! o `FakeMemory` cobre os testes do kernel.

/// Porta de memória do agente.
///
/// Implementada in-process sobre o `knudge-core` (E03).
pub trait Memory {}
