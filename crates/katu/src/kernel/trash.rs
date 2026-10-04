//! Lixeira na TUI (E10-T07/E06-T09): listar, restaurar e **esvaziar** com challenge.
//!
//! Restaurar é **sempre** permitido (recuperável). Esvaziar é **destrutivo**: exige um
//! challenge-and-response (§33) — só um humano assina e a remoção é permanente.

use katu_core::api::{Event as Update, TrashEntry};
use katu_core::diag::{Level, events};
use katu_tools::trash;

use super::{BusSink, Kernel};

impl Kernel<'_> {
    /// Lista a lixeira do projeto para a UI (E10-T07/E06-T09).
    pub(super) fn trash_list(&self) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_TRASH, "trash::trash_list");
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
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_TRASH, "trash::restore");
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
    pub(super) fn empty_trash(&self, sink: &mut BusSink<'_>) -> Vec<Update> {
        let _span = katu_core::fn_span!(Level::Trace, events::TOOL_TRASH, "trash::empty_trash");
        let count = trash::list(self.fs, self.runtime.root()).len();
        if count == 0 {
            return vec![Update::Info("lixeira vazia".to_string())];
        }
        let scope = format!("{count} item(ns)");
        if sink.ask("trash", "trash-empty", &scope).is_none() {
            return vec![Update::Info("esvaziamento cancelado".to_string())];
        }
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
