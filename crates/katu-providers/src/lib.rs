//! `katu-providers` — E12: camada de providers (commodity).
//!
//! Caminho built-in first-party (`opencode go/zen` + `llama.cpp`) e demais providers via
//! GDK/declarativo, ou ignorados (DF8). É o **único** crate que fala com modelos; o modelo é
//! cliente do plano de dados, nunca substrato do loop.

#![forbid(unsafe_code)]
