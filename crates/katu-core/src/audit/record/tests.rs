//! Testes da projeção de evento em linha de auditoria.

use super::AuditRecord;
use crate::kernel::Event;

#[test]
fn user_message_becomes_searchable_text() {
    let record = AuditRecord::from_event(
        3,
        &Event::UserMessage {
            text: "Corrige o parser".to_string(),
        },
    );
    assert_eq!(record.seq, 3);
    assert_eq!(record.kind, "user");
    assert_eq!(record.text, "Corrige o parser");
}
