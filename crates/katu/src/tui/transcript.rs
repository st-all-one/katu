//! Transcrição durável em ficheiro + vista read-only (E10-T05).
//!
//! O ficheiro `.katu/transcript.md` é a projeção do **log** (durável); a vista da TUI lê-o de volta,
//! read-only. O painel de atividade (efémero) nunca entra aqui (§50.3).

use std::path::PathBuf;

use katu_core::kernel::katu_dir;
use katu_core::ports::Fs;
use katu_tui::Update;

use super::AgentHandler;

impl AgentHandler<'_> {
    /// Caminho do ficheiro de transcrição (`<root>/.katu/transcript.md`).
    fn transcript_path(&self) -> PathBuf {
        let _span = katu_core::trace_fn!("tui::transcript::transcript_path");

        katu_dir(self.runtime.root()).join("transcript.md")
    }

    /// Escreve a transcrição durável no ficheiro; devolve um `Update` de erro, se falhar.
    pub(super) fn write_transcript(&self) -> Option<Update> {
        let _span = katu_core::trace_fn!("tui::transcript::write_transcript");

        let lines = match self.runtime.transcript() {
            Ok(lines) => lines,
            Err(error) => return Some(Update::Error(format!("transcrição: {error}"))),
        };
        let path = self.transcript_path();
        if let Some(parent) = path.parent()
            && let Err(error) = self.fs.create_dir_all(parent)
        {
            return Some(Update::Error(format!("transcrição: {error}")));
        }
        let mut body = lines.join("\n").into_bytes();
        body.push(b'\n');
        match self.fs.write_atomic(&path, &body) {
            Ok(()) => None,
            Err(error) => Some(Update::Error(format!("transcrição: {error}"))),
        }
    }

    /// Serve a vista read-only: garante o ficheiro e lê-o de volta (E10-T05).
    pub(super) fn transcript_view(&self) -> Vec<Update> {
        let _span = katu_core::trace_fn!("tui::transcript::transcript_view");

        let mut updates = Vec::new();
        if let Some(error) = self.write_transcript() {
            updates.push(error);
        }
        match self.fs.read(&self.transcript_path()) {
            Ok(bytes) => updates.push(Update::Transcript(split_lines(&bytes))),
            Err(error) => updates.push(Update::Error(format!("transcrição: {error}"))),
        }
        updates
    }
}

/// Divide o ficheiro em linhas (lossy: nunca rebenta com bytes inválidos).
fn split_lines(bytes: &[u8]) -> Vec<String> {
    let _span = katu_core::trace_fn!("tui::transcript::split_lines");

    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_string)
        .collect()
}
