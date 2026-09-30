//! Suíte de conformidade da porta `Memory` (E03-T05).
//!
//! Corre contra qualquer backend — o [`FakeMemory`](super::FakeMemory) agora, o adaptador
//! in-process (E03-T02) e o futuro adaptador MCP (E08) — assertando a **forma** do contrato
//! (§16.6, paridade entre backends). Não testa o scoring (E18); testa que a API responde com os
//! tipos e as invariantes certas.

use super::{Memory, NoteType, PreWriteOutcome, PreWriteReq, RecallReq, SessionEndReq};

/// Pedido de escrita válido, reutilizado pelos testes de conformidade.
fn sample_write() -> PreWriteReq {
    let _span = crate::trace_fn!("memory::conformance::sample_write");

    PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Decision,
        anchor: None,
        body: String::new(),
    }
}

/// Asserta o contrato da porta contra um backend **saudável**.
///
/// # Errors
/// Se alguma invariante do contrato for violada (backend indisponível, id vazio, `pre_write` não
/// determinístico).
pub fn assert_contract(memory: &dyn Memory) -> Result<(), Box<dyn std::error::Error>> {
    let _span = crate::trace_fn!("memory::conformance::assert_contract");

    let status = memory.status()?;
    if status.backend.trim().is_empty() {
        return Err("status.backend vazio".into());
    }

    let req = sample_write();
    let first = memory.pre_write(&req)?;
    let second = memory.pre_write(&req)?;
    if first != second {
        return Err("pre_write não é determinístico para o mesmo pedido".into());
    }
    if matches!(first, PreWriteOutcome::Create) {
        let note = memory.record(&req)?;
        if note.as_str().trim().is_empty() {
            return Err("record devolveu uma referência vazia".into());
        }
    }

    let hits = memory.search(&RecallReq {
        query: "cache".to_string(),
        limit: 5,
    })?;
    for hit in &hits {
        if hit.note.as_str().trim().is_empty() || hit.statement.trim().is_empty() {
            return Err("hit com nota ou afirmação vazia".into());
        }
    }

    memory.session_end(&SessionEndReq {
        task: None,
        actor: "conformance".to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::assert_contract;
    use crate::memory::FakeMemory;

    #[test]
    fn fake_memory_satisfies_the_contract() -> Result<(), Box<dyn std::error::Error>> {
        assert_contract(&FakeMemory::default())
    }
}
