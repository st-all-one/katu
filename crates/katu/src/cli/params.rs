//! `--params` e `--batch` (E20-T08): entrada estruturada, **sem inferência**.
//!
//! `--params '{…}'` representa toda a config de um comando e é **exclusivo** com as flags
//! explícitas. `--batch <ficheiro|->` processa JSONL (uma linha = um item). `-` lê `stdin`.

use std::io::Read;

use katu_core::error::Error;
use serde::de::DeserializeOwned;

/// Lê o conteúdo de `--params`/`--batch` (`-` = `stdin`).
pub(crate) fn source(raw: &str) -> Result<String, Error> {
    if raw == "-" {
        return read_stdin();
    }
    Ok(raw.to_owned())
}

/// Lê um lote (`-` = `stdin`) e devolve as linhas não vazias.
pub(crate) fn batch_lines(path: &str) -> Result<Vec<String>, Error> {
    let text = if path == "-" {
        read_stdin()?
    } else {
        std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Desserializa um objeto JSON, recusando campos desconhecidos (fail-closed).
pub(crate) fn parse<T: DeserializeOwned>(text: &str) -> Result<T, Error> {
    serde_json::from_str(text).map_err(|err| Error::invalid_input(format!("JSON inválido: {err}")))
}

/// Lê todo o `stdin`.
fn read_stdin() -> Result<String, Error> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|err| Error::io("<stdin>", err))?;
    Ok(buffer)
}
