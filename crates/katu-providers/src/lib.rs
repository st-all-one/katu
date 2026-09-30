//! `katu-providers` — E12: camada de providers (commodity).
//!
//! Caminho built-in first-party (`opencode go/zen` + `llama.cpp`) e demais providers via
//! GDK/declarativo, ou ignorados (DF8). É o **único** crate que fala com modelos; o modelo é
//! cliente do plano de dados, nunca substrato do loop.
//!
//! ## Forma
//!
//! - A **porta** (`Provider`, `ProviderEvent`, `ProviderError`) vive em `katu-core::provider` — o
//!   núcleo define o contrato sem depender deste crate (firewall LLM-free, E12-T01).
//! - Aqui ficam os **adaptadores**: [`OpenCode`] (hot path), [`Llama`] (local), [`FakeProvider`]
//!   (testes) e o transporte ([`Transport`]) com [`UreqTransport`] (real) e [`MockTransport`]
//!   (testes sem rede).
//! - O dialeto `chat/completions` é partilhado por `opencode` e `llama-server` e consumido de
//!   forma **incremental** (SSE linha a linha, tool calls completas) — latência primeiro.

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "módulos internos usam pub(crate); a API pública é a reexportação em `lib.rs`"
)]

pub mod fake;
pub mod http;
pub mod llama;
pub mod openai;
pub mod opencode;
pub mod retry;
pub mod sse;
pub mod transport;
pub mod usage;

pub use fake::{FakeProvider, Turn};
pub use http::UreqTransport;
pub use llama::{Llama, LlamaConfig};
pub use openai::Endpoint;
pub use opencode::{Dialect, OpenCode, OpenCodeConfig};
pub use retry::RetryPolicy;
pub use transport::{
    ChunkSink, HttpMeta, HttpRequest, Method, MockTransport, Transport, TransportError,
};
pub use usage::{Cost, Price, PriceTable};

#[cfg(test)]
mod tests;
