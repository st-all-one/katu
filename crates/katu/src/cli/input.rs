//! Convenção de entrada da superfície v2 (E20-T05): o posicional é **conteúdo**.
//!
//! `-` lê `stdin`; ausente + `stdin` não-TTY lê `stdin`; ausente + TTY é `invalid_input` — nunca
//! se inventa conteúdo (I1, fail-closed).

use std::io::{IsTerminal, Read};

use katu_core::error::Error;

/// Resolve o *body* posicional a partir do literal, de `stdin` ou de um *pipe*.
///
/// # Errors
/// [`Error::invalid_input`] quando não há conteúdo e o `stdin` é um TTY; [`Error::io`] quando a
/// leitura de `stdin` falha.
pub(crate) fn resolve(body: Option<&str>) -> Result<String, Error> {
    match body {
        Some("-") => read_stdin(),
        Some(text) => Ok(text.to_owned()),
        None if std::io::stdin().is_terminal() => Err(Error::invalid_input(
            "conteúdo ausente: passe um body, `-` ou use um pipe",
        )),
        None => read_stdin(),
    }
}

/// Lê todo o `stdin` como texto (UTF-8).
fn read_stdin() -> Result<String, Error> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| Error::io("<stdin>", error))?;
    Ok(buffer)
}
