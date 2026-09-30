//! Accessores da vista da transcrição durável (E10-T05).
//!
//! Vive num módulo filho para manter `app.rs` sob o limite de linhas; o campo `viewer` é privado
//! e só estes accessores o expõem ao render.

use super::App;

impl App {
    /// A vista da transcrição está aberta (E10-T05).
    #[must_use]
    pub fn viewer_open(&self) -> bool {
        self.viewer.is_open()
    }

    /// Linhas da transcrição durável.
    #[must_use]
    pub fn viewer_lines(&self) -> &[String] {
        self.viewer.lines()
    }

    /// Deslocamento da vista da transcrição.
    #[must_use]
    pub const fn viewer_scroll(&self) -> u16 {
        self.viewer.scroll()
    }
}
