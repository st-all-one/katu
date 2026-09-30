//! Testes do índice invertido (termos, frases, filtros).

use super::{Index, Query};
use crate::audit::AuditRecord;

fn record(seq: u64, kind: &'static str, text: &str) -> AuditRecord {
    AuditRecord {
        seq,
        kind,
        tool: String::new(),
        path: String::new(),
        status: String::new(),
        rule: String::new(),
        text: text.to_string(),
    }
}

fn matches(index: &Index, records: &[AuditRecord], query: &str) -> Vec<u32> {
    let parsed = Query::parse(query);
    let mut lines: Vec<u32> = parsed
        .groups
        .iter()
        .flat_map(|group| index.matches(records, group))
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

#[test]
fn terms_and_phrases_intersect() {
    let records = vec![
        record(1, "user", "corrige o parser de toon"),
        record(2, "user", "parser de yaml"),
    ];
    let index = Index::build(&records);
    assert_eq!(matches(&index, &records, "toon"), vec![0]);
    assert_eq!(matches(&index, &records, "parser"), vec![0, 1]);
    assert_eq!(matches(&index, &records, "\"parser de toon\""), vec![0]);
    assert_eq!(matches(&index, &records, "parser yaml"), vec![1]);
    assert_eq!(
        matches(&index, &records, "kind:assistant"),
        Vec::<u32>::new()
    );
}

#[test]
fn or_separates_groups() {
    let records = vec![record(1, "user", "alfa"), record(2, "user", "beta")];
    let index = Index::build(&records);
    assert_eq!(matches(&index, &records, "alfa OR beta"), vec![0, 1]);
}
