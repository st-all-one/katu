//! Testes de retry (E12-T04): transporte instável e limites permanentes.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use katu_core::provider::{CollectSink, Flow, Provider, StopReason};

use super::{TEXT_STREAM, request};
use crate::opencode::{OpenCode, OpenCodeConfig};
use crate::retry::RetryPolicy;
use crate::transport::{ChunkSink, HttpMeta, HttpRequest, Transport, TransportError};

/// Metadados de resposta.
fn meta(status: u16, bytes: u64) -> HttpMeta {
    HttpMeta {
        status,
        headers: Vec::new(),
        bytes,
    }
}

/// Retry rápido para testes (sem esperas de centenas de ms).
fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_retries: 1,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(5),
    }
}

/// Entrega `body` em fragmentos de `size` bytes.
fn pump(body: &[u8], size: usize, sink: &mut ChunkSink<'_>) {
    for fragment in body.chunks(size) {
        if matches!(sink(fragment), Flow::Break) {
            break;
        }
    }
}

/// Transporte que falha `fails` vezes com `status` e depois serve `body`.
struct FlakyTransport {
    fails: Mutex<u32>,
    status: u16,
    body: Vec<u8>,
}

impl Transport for FlakyTransport {
    fn send(
        &self,
        _request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError> {
        {
            let mut fails = self.fails.lock().unwrap_or_else(PoisonError::into_inner);
            if *fails > 0 {
                *fails = fails.saturating_sub(1);
                return Ok(meta(self.status, 0));
            }
        }
        pump(&self.body, 16, sink);
        let bytes = u64::try_from(self.body.len()).unwrap_or(u64::MAX);
        Ok(meta(200, bytes))
    }
}

/// Transporte que conta chamadas e devolve sempre `status` + `body`.
struct CountingTransport {
    calls: Arc<Mutex<u32>>,
    status: u16,
    body: Vec<u8>,
}

impl Transport for CountingTransport {
    fn send(
        &self,
        _request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError> {
        {
            let mut calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
            *calls = calls.saturating_add(1);
        }
        pump(&self.body, 64, sink);
        let bytes = u64::try_from(self.body.len()).unwrap_or(u64::MAX);
        Ok(meta(self.status, bytes))
    }
}

#[test]
fn retries_a_transient_status_before_any_event() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FlakyTransport {
        fails: Mutex::new(1),
        status: 503,
        body: TEXT_STREAM.as_bytes().to_vec(),
    };
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    )
    .with_retry(fast_retry());
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request("m"), &mut sink)?;
    assert_eq!(sink.text, "Ola");
    assert_eq!(outcome.stop, StopReason::EndTurn);
    Ok(())
}

#[test]
fn account_limit_is_permanent_and_not_retried() {
    let calls = Arc::new(Mutex::new(0_u32));
    let transport = CountingTransport {
        calls: Arc::clone(&calls),
        status: 429,
        body: br#"{"error":{"type":"FreeTierError"}}"#.to_vec(),
    };
    let provider = OpenCode::new(
        transport,
        OpenCodeConfig::at("https://example.invalid/v1", "k"),
    )
    .with_retry(fast_retry());
    let mut sink = CollectSink::default();
    assert!(provider.stream(&request("m"), &mut sink).is_err());
    assert_eq!(*calls.lock().unwrap_or_else(PoisonError::into_inner), 1);
}
