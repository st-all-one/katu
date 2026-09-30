//! Controlo de modelo/pensamento no kernel (E12-T10): evento, estado e *resume*.

use super::Session;
use crate::kernel::{Control, Event};
use crate::ports::MemFs;
use crate::provider::Thinking;
use std::path::Path;

#[test]
fn control_is_logged_and_survives_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    {
        let mut session = Session::open(&fs, dir)?;
        session.apply(&Event::TurnStart { turn: 1 })?;
        session.apply(&Event::Control {
            control: Control::SetModel {
                model: "model-x".to_string(),
            },
        })?;
        session.apply(&Event::Control {
            control: Control::SetThinking {
                thinking: Thinking::Low,
            },
        })?;
    }
    let reopened = Session::open(&fs, dir)?;
    assert_eq!(reopened.state().control.model.as_deref(), Some("model-x"));
    assert_eq!(reopened.state().control.thinking, Thinking::Low);
    Ok(())
}
