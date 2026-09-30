//! Accessores da vista da transcrição durável (E10-T05).
//!
//! Vive num módulo filho para manter `app.rs` sob o limite de linhas; o campo `viewer` é privado
//! e só estes accessores o expõem ao render.

use super::App;

impl App {
    /// A vista da transcrição está aberta (E10-T05).
    #[must_use]
    pub fn viewer_open(&self) -> bool {
        let _span = katu_core::trace_fn!("app::viewer::viewer_open");

        self.viewer.is_open()
    }

    /// Linhas da transcrição durável.
    #[must_use]
    pub fn viewer_lines(&self) -> &[String] {
        let _span = katu_core::trace_fn!("app::viewer::viewer_lines");

        self.viewer.lines()
    }

    /// Deslocamento da vista da transcrição.
    #[must_use]
    pub const fn viewer_scroll(&self) -> u16 {
        self.viewer.scroll()
    }
}
