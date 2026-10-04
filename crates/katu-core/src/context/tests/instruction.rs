//! A instrução corrente nunca sai do contexto (regressão do «Sem tarefa no pedido»).

use super::{budget, read_use};
use crate::context::assemble;
use crate::error::ToolOutcome;
use crate::kernel::{CallId, Event, Message, Visibility};

#[test]
fn the_instruction_survives_a_tool_flood() -> Result<(), Box<dyn std::error::Error>> {
    // O sufixo evictava a mensagem do utilizador quando os resultados das tools enchiam o
    // orçamento; o modelo perdia a tarefa. A instrução é pinada.
    let use_ = read_use()?;
    let goal = "escreve o resumo em teste.md";
    let mut events = vec![Event::UserMessage {
        text: goal.into(),
        visibility: Visibility::User,
    }];
    for call in 0..40_u32 {
        events.push(Event::ToolCall {
            call: CallId::new(format!("c{call}")),
            tool: use_.clone(),
        });
        events.push(Event::ToolResult {
            call: CallId::new(format!("c{call}")),
            outcome: ToolOutcome::Ok,
            delta: Some("x".repeat(400)),
        });
    }
    let context = assemble(&events, budget(200));
    assert!(
        context
            .messages
            .iter()
            .any(|message| matches!(message, Message::User { text, .. } if text == goal)),
        "a instrução tem de sobreviver ao dilúvio de tools: {:?}",
        context.messages
    );
    assert!(
        context.raw_tokens <= 200,
        "orçamento excedido: {}",
        context.raw_tokens
    );
    Ok(())
}
