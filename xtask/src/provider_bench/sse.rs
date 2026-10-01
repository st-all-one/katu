//! A/B do parser SSE (P-04) com **réplica congelada** do anterior.
//!
//! O caminho quente de um turno é um `data:` por delta; o parser antigo alocava duas `String` por
//! delta (a linha e o payload). Aqui a réplica mantém esse comportamento e o parser de produção
//! corre ao lado, sobre o **mesmo** corpus e os **mesmos** fragmentos — e o teste de CI exige que
//! ambos produzam exatamente os mesmos eventos (a otimização não pode mudar bytes).
#![allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]

use std::time::Instant;

use katu_core::provider::Flow;
use katu_providers::sse::SseParser;

/// Réplica do parser anterior a P-04 (duas alocações por delta).
#[derive(Debug, Default)]
struct LegacyParser {
    pending: Vec<u8>,
    data: String,
}

impl LegacyParser {
    fn push(&mut self, chunk: &[u8], on_data: &mut dyn FnMut(&str) -> Flow) -> Flow {
        self.pending.extend_from_slice(chunk);
        let mut consumed = 0_usize;
        let mut flow = Flow::Continue;
        while let Some(offset) = self
            .pending
            .get(consumed..)
            .and_then(|rest| rest.iter().position(|byte| *byte == b'\n'))
        {
            let end = consumed.saturating_add(offset);
            let line = strip_cr(self.pending.get(consumed..end).unwrap_or(&[]));
            if line.is_empty() {
                let payload = self
                    .data
                    .strip_suffix('\n')
                    .unwrap_or(&self.data)
                    .to_string();
                if !payload.is_empty() {
                    flow = on_data(&payload);
                    self.data.clear();
                    if matches!(flow, Flow::Break) {
                        consumed = end.saturating_add(1);
                        break;
                    }
                }
            } else if let Some(value) = legacy_data_value(line) {
                self.data.push_str(&value);
                self.data.push('\n');
            }
            consumed = end.saturating_add(1);
        }
        let keep = consumed.min(self.pending.len());
        self.pending.drain(..keep);
        flow
    }
}

/// Versão anterior: devolve `String` (uma alocação por linha `data:`).
fn legacy_data_value(line: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(line).ok()?;
    let value = text.strip_prefix("data:")?;
    let value = value.strip_prefix(' ').unwrap_or(value);
    Some(value.to_string())
}

/// Remove o `\r` final, se existir (igual em ambos os parsers).
fn strip_cr(line: &[u8]) -> &[u8] {
    match line.last() {
        Some(b'\r') => line.get(..line.len().saturating_sub(1)).unwrap_or(&[]),
        _ => line,
    }
}

/// Resultado do A/B: eventos vistos, nanos por passagem e ganho.
pub(super) struct SseAb {
    /// Eventos de dados entregues (têm de ser iguais nos dois).
    pub events: usize,
    /// Fragmentos em que o corpus foi dividido.
    pub chunks: usize,
    /// Nanos por passagem no parser de produção (mediana).
    pub production_nanos: u64,
    /// Nanos por passagem na réplica anterior (mediana).
    pub legacy_nanos: u64,
    /// Mínimo das passagens no parser de produção (o sinal com menos ruído).
    pub production_min_nanos: u64,
    /// Mínimo das passagens na réplica anterior.
    pub legacy_min_nanos: u64,
}

/// Corre o corpus pelos dois parsers e mede (mediana de `reps`).
pub(super) fn ab(corpus: &str, chunk: usize, reps: u32) -> SseAb {
    let chunk = chunk.max(1);
    let mut events = 0_usize;
    let mut production = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    let mut legacy = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    for rep in 0..reps {
        // A ordem **alterna**: quem corre primeiro beneficia do cache quente, e uma ordem fixa
        // atribuiria essa vantagem sempre ao mesmo parser.
        let (first, second) = if rep.checked_rem(2) == Some(0) {
            (run_production(corpus, chunk), run_legacy(corpus, chunk))
        } else {
            let old = run_legacy(corpus, chunk);
            let new = run_production(corpus, chunk);
            (new, old)
        };
        events = first.0;
        assert_eq!(first.0, second.0, "os dois parsers divergem");
        production.push(first.1);
        legacy.push(second.1);
    }
    production.sort_unstable();
    legacy.sort_unstable();
    SseAb {
        events,
        chunks: corpus.len().div_ceil(chunk),
        production_nanos: median(&production),
        legacy_nanos: median(&legacy),
        production_min_nanos: production.first().copied().unwrap_or(0),
        legacy_min_nanos: legacy.first().copied().unwrap_or(0),
    }
}

/// Passagem pelo parser de produção.
fn run_production(corpus: &str, chunk: usize) -> (usize, u64) {
    let start = Instant::now();
    let mut parser = SseParser::new();
    let mut events = 0_usize;
    for fragment in corpus.as_bytes().chunks(chunk) {
        let flow = parser.push(fragment, &mut |payload| {
            events = events.saturating_add(1);
            std::hint::black_box(payload.len());
            Flow::Continue
        });
        if matches!(flow, Flow::Break) {
            break;
        }
    }
    let nanos = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (events, nanos)
}

/// Passagem pela réplica congelada.
fn run_legacy(corpus: &str, chunk: usize) -> (usize, u64) {
    let start = Instant::now();
    let mut parser = LegacyParser::default();
    let mut events = 0_usize;
    for fragment in corpus.as_bytes().chunks(chunk) {
        let flow = parser.push(fragment, &mut |payload| {
            events = events.saturating_add(1);
            std::hint::black_box(payload.len());
            Flow::Continue
        });
        if matches!(flow, Flow::Break) {
            break;
        }
    }
    let nanos = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (events, nanos)
}

/// Mediana de amostras (já ordenadas).
fn median(sorted: &[u64]) -> u64 {
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{LegacyParser, SseParser, ab};
    use crate::provider_bench::report;
    use katu_core::provider::Flow;

    #[test]
    fn both_parsers_see_the_same_events() {
        let corpus = "data: a\n\ndata: b\n\ndata: {\"x\":1}\n\n";
        let mut production = Vec::new();
        let mut legacy = Vec::new();
        let mut parser = SseParser::new();
        parser.push(corpus.as_bytes(), &mut |payload| {
            production.push(payload.to_string());
            Flow::Continue
        });
        let mut old = LegacyParser::default();
        old.push(corpus.as_bytes(), &mut |payload| {
            legacy.push(payload.to_string());
            Flow::Continue
        });
        assert_eq!(production, legacy);
        assert_eq!(production, vec!["a", "b", "{\"x\":1}"]);
    }

    #[test]
    fn the_ab_reports_the_same_events_for_both() {
        let corpus = report::corpus();
        let outcome = ab(&corpus, 4096, 3);
        // 512 deltas de texto + tool call + usage + `[DONE]`: um evento por `data:`.
        assert_eq!(outcome.events, 515, "um evento por `data:`");
        assert!(outcome.chunks > 1);
    }
}
