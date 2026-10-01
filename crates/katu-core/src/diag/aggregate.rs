//! Sink agregador (E19-T02/E19-T03): contagens e durações por evento **e por função**, com
//! percentis.
//!
//! Agrega apenas registos de [`Kind::SpanEnd`](super::Kind) (têm duração), com chave
//! `(event, function)` — onde `function` é o rótulo opcional de [`fn_span!`](crate::fn_span). O
//! instantâneo é **determinístico** — ordem canónica por identificador de evento e, depois, por
//! função — e destina-se a ser gravado como artefacto com base `measured` (DF5). Sem alocação no
//! caminho quente além do `push` do vetor de durações; os percentis calculam-se no *dump*, não na
//! recolha.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use super::{Kind, Record, Sink};
use crate::stats::{Summary, percentile};

/// Resumo determinístico das durações de um evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSummary {
    /// Identificador do catálogo.
    pub event: &'static str,
    /// Rótulo de função (`fn_span!`), quando o span o declara; `None` caso contrário.
    pub function: Option<&'static str>,
    /// Número de amostras.
    pub count: u64,
    /// Soma das durações (ns).
    pub total_nanos: u64,
    /// Mínimo (ns).
    pub min_nanos: u64,
    /// Mediana (ns).
    pub p50_nanos: u64,
    /// Percentil 95 (ns).
    pub p95_nanos: u64,
    /// Percentil 99 (ns).
    pub p99_nanos: u64,
    /// Máximo (ns).
    pub max_nanos: u64,
    /// Limite inferior do IC 95 % da média (ns).
    pub ci95_low_nanos: u64,
    /// Limite superior do IC 95 % da média (ns).
    pub ci95_high_nanos: u64,
}

#[derive(Debug, Default, Clone)]
struct Stat {
    durations: Vec<u64>,
    total_nanos: u64,
}

/// Chave de agregação: identificador de evento + rótulo de função (opcional).
type StatKey = (&'static str, Option<&'static str>);

/// Sink que agrega contagens e durações por evento (e por função) (thread-safe).
#[derive(Debug, Default)]
pub struct AggregatingSink {
    stats: Mutex<BTreeMap<StatKey, Stat>>,
}

impl Sink for AggregatingSink {
    fn record(&self, record: &Record<'_>) {
        if record.kind != Kind::SpanEnd {
            return;
        }
        let Some(nanos) = record.duration_nanos else {
            return;
        };
        let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
        let mut stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
        let stat = stats.entry((record.event, record.function)).or_default();
        stat.total_nanos = stat.total_nanos.saturating_add(nanos);
        stat.durations.push(nanos);
    }
}

impl AggregatingSink {
    /// Instantâneo determinístico (ordem canónica por evento, percentis calculados agora).
    ///
    /// O `Mutex` é largado **antes** de resumir: os spans de `stats` que `summarize` abre voltariam
    /// ao próprio sink (recursão no lock).
    #[must_use]
    pub fn snapshot(&self) -> Vec<EventSummary> {
        let entries: Vec<(StatKey, Stat)> = {
            let stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
            stats
                .iter()
                .map(|(key, stat)| (*key, stat.clone()))
                .collect()
        };
        entries
            .iter()
            .map(|((event, function), stat)| summarize(event, *function, stat))
            .collect()
    }

    /// Número de chaves `(evento, função)` distintas agregadas.
    #[must_use]
    pub fn events(&self) -> usize {
        self.stats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Total de amostras agregadas.
    #[must_use]
    pub fn samples(&self) -> u64 {
        let stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
        stats
            .values()
            .fold(0_u64, |acc, stat| acc.saturating_add(len_u64(stat)))
    }
}

fn len_u64(stat: &Stat) -> u64 {
    u64::try_from(stat.durations.len()).unwrap_or(u64::MAX)
}

fn summarize(event: &'static str, function: Option<&'static str>, stat: &Stat) -> EventSummary {
    let mut sorted = stat.durations.clone();
    sorted.sort_unstable();
    let summary = Summary::from_sorted(&sorted);
    EventSummary {
        event,
        function,
        count: len_u64(stat),
        total_nanos: stat.total_nanos,
        min_nanos: sorted.first().copied().unwrap_or(0),
        p50_nanos: summary.p50,
        p95_nanos: summary.p95,
        p99_nanos: percentile(&sorted, 9_900),
        max_nanos: sorted.last().copied().unwrap_or(0),
        ci95_low_nanos: summary.ci95_low,
        ci95_high_nanos: summary.ci95_high,
    }
}

#[cfg(test)]
mod tests {
    use super::{AggregatingSink, percentile};
    use crate::diag::{Kind, Level, Record, Sink, events};

    fn span_end(event: &'static str, nanos: u128) -> Record<'static> {
        Record {
            level: Level::Info,
            event,
            function: None,
            kind: Kind::SpanEnd,
            duration_nanos: Some(nanos),
            fields: &[],
        }
    }

    fn point(event: &'static str) -> Record<'static> {
        Record {
            level: Level::Info,
            event,
            function: None,
            kind: Kind::Event,
            duration_nanos: None,
            fields: &[],
        }
    }

    #[test]
    fn aggregates_and_sorts_events() {
        let sink = AggregatingSink::default();
        sink.record(&span_end(events::KERNEL_STEP, 100));
        sink.record(&span_end(events::KERNEL_STEP, 300));
        sink.record(&span_end(events::POLICY_EVALUATE, 50));
        sink.record(&point(events::TOOL_OK));
        let snapshot = sink.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot.first().map(|s| s.event), Some(events::KERNEL_STEP));
        assert_eq!(
            snapshot.get(1).map(|s| s.event),
            Some(events::POLICY_EVALUATE)
        );
        let step = snapshot.first();
        assert_eq!(step.map(|s| s.count), Some(2));
        assert_eq!(step.map(|s| s.total_nanos), Some(400));
        assert_eq!(step.map(|s| s.min_nanos), Some(100));
        assert_eq!(step.map(|s| s.max_nanos), Some(300));
        assert_eq!(sink.samples(), 3);
    }

    #[test]
    fn percentile_is_nearest_rank() {
        let data: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&data, 5_000), 50);
        assert_eq!(percentile(&data, 9_500), 95);
        assert_eq!(percentile(&data, 9_900), 99);
        assert_eq!(percentile(&[], 5_000), 0);
    }

    #[test]
    fn groups_by_function_label() {
        let sink = AggregatingSink::default();
        let with_fn = |function: &'static str, nanos: u128| Record {
            level: Level::Info,
            event: events::KERNEL_STEP,
            function: Some(function),
            kind: Kind::SpanEnd,
            duration_nanos: Some(nanos),
            fields: &[],
        };
        sink.record(&with_fn("a::one", 10));
        sink.record(&with_fn("a::two", 20));
        sink.record(&span_end(events::KERNEL_STEP, 30));
        let snapshot = sink.snapshot();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot.first().map(|s| s.function), Some(None));
        assert_eq!(snapshot.get(1).map(|s| s.function), Some(Some("a::one")));
        assert_eq!(snapshot.get(2).map(|s| s.function), Some(Some("a::two")));
    }

    #[test]
    fn snapshot_is_deterministic() {
        let sink = AggregatingSink::default();
        for value in [5_u128, 1, 3, 2, 4] {
            sink.record(&span_end(events::FS_READ, value));
        }
        assert_eq!(sink.snapshot(), sink.snapshot());
        assert_eq!(sink.events(), 1);
    }
}
