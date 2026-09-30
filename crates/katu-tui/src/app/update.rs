//! Injeção de atualizações da borda no [`App`] (E10-T02/T05/T06/T10).
//!
//! Vive num módulo filho para manter `app.rs` sob o teto de linhas. As atualizações são o único
//! caminho pelo qual o executor devolve resultados à UI; o render continua **puro**.

use crate::entry::{Entry, Role, Status};
use crate::message::Update;

use super::App;

impl App {
    /// Injeta o resultado do executor.
    pub fn apply_update(&mut self, update: Update) {
        match update {
            Update::Assistant(text) => self.push(Role::Assistant, text),
            Update::Tool(text) => self.push(Role::Tool, text),
            Update::Info(text) => self.push(Role::Info, text),
            Update::Error(text) => {
                self.status = Status::Failure(text.clone());
                self.push(Role::Error, text);
            }
            Update::Phase(phase) => self.phase = phase,
            Update::Live(live) => self.apply_live(live),
            Update::Models(models) => self.controls.set_models(models),
            Update::ThinkingOptions(options) => self.apply_thinking_options(options),
            Update::NextAction(action) => self.next_action = Some(action),
            Update::Usage(text) => self.usage = Some(text),
            Update::Trash(items) => self.trash.set_items(items),
            Update::Transcript(lines) => self.viewer.set_lines(lines),
            Update::Cancelled => {
                self.clear_live();
                self.status = Status::Message("turno cancelado".to_string());
            }
            Update::Done => {
                self.clear_live();
                self.status = Status::Idle;
            }
        }
    }

    /// Acrescenta uma entrada (ignora texto vazio) e volta ao fundo.
    pub(super) fn push(&mut self, role: Role, text: String) {
        if !text.is_empty() {
            self.transcript.push(Entry { role, text });
            self.scroll = 0;
        }
    }
}
