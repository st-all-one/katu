//! Lixeira na TUI (E10-T07/E06-T09): listar, restaurar e **esvaziar** com challenge.
//!
//! Restaurar é **sempre** permitido (recuperável). Esvaziar é **destrutivo**: exige um
//! challenge-and-response (§33) — só um humano assina e a remoção é permanente.

use katu_core::diag::{Level, events};
use katu_tools::trash;
use katu_tui::{ChallengePrompt, Painter, TrashEntry, Update};

use super::AgentHandler;

impl AgentHandler<'_> {
    /// Lista a lixeira do projeto para a UI (E10-T07/E06-T09).
    pub(super) fn trash_list(&self) -> Vec<Update> {
        let items = trash::list(self.fs, self.runtime.root())
            .into_iter()
            .map(|item| TrashEntry {
                original: item.original,
                stored: item.stored,
            })
            .collect();
        vec![Update::Trash(items)]
    }

    /// Restaura um item da lixeira e devolve a lista atualizada (E06-T09).
    pub(super) fn restore(&self, token: &str) -> Vec<Update> {
        match trash::restore(self.fs, self.runtime.root(), token) {
            Ok(path) => {
                let mut updates = vec![Update::Info(format!("restaurado: {}", path.display()))];
                updates.extend(self.trash_list());
                updates
            }
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }

    /// Esvazia a lixeira **permanentemente**, após challenge humano (E10-T07, §33).
    pub(super) fn empty_trash(&self, painter: &mut Painter<'_>) -> Vec<Update> {
        let count = trash::list(self.fs, self.runtime.root()).len();
        if count == 0 {
            return vec![Update::Info("lixeira vazia".to_string())];
        }
        let request = ChallengePrompt {
            tool: "trash".to_string(),
            rule: "trash-empty".to_string(),
            scope: format!("{count} item(ns)"),
        };
        let granted_by = self.granted_by();
        let Some(_signature) = painter.challenge(request, &granted_by) else {
            return vec![Update::Info("esvaziamento cancelado".to_string())];
        };
        match trash::empty(self.fs, self.runtime.root()) {
            Ok(removed) => {
                katu_core::event!(Level::Warn, events::TUI_TRASH_EMPTY);
                let mut updates = vec![Update::Info(format!(
                    "lixeira esvaziada: {removed} item(ns)"
                ))];
                updates.extend(self.trash_list());
                updates
            }
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }
}
