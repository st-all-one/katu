//! Testes do actor do kernel (E12-T03): linha de uso/custo.

use katu_core::evidence::EvidenceBasis;
use katu_core::provider::{StopReason, TokenUsage};
use katu_providers::{Price, PriceTable};

use super::usage_line;
use crate::agent::{Termination, TurnReport};

fn report(usage: Option<TokenUsage>) -> TurnReport {
    TurnReport {
        model: "fake".to_string(),
        steps: 1,
        text: String::new(),
        calls: 0,
        usage,
        cancelled: false,
        stop: StopReason::EndTurn,
        termination: Termination::Natural,
    }
}

#[test]
fn usage_line_shows_tokens_and_cost_when_priced() {
    let mut usage = TokenUsage::new(EvidenceBasis::ProviderReported);
    usage.input = Some(1_000);
    usage.output = Some(500);
    let mut prices = PriceTable::new();
    prices.set(
        "m",
        Price {
            input: 1_000_000,
            output: 3_000_000,
            cached_input: 0,
        },
    );
    let line = usage_line("m", &report(Some(usage)), &prices).unwrap_or_default();
    assert!(line.contains("in 1000"), "{line}");
    assert!(line.contains("out 500"), "{line}");
    assert!(line.contains("custo"), "{line}");
}

#[test]
fn usage_line_without_usage_is_none() {
    assert!(usage_line("m", &report(None), &PriceTable::new()).is_none());
}
