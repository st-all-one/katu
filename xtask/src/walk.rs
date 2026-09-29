//! Percurso de ficheiros partilhado pelos checks do `xtask`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Recolhe recursivamente ficheiros com a extensão dada sob `dir`.
pub(crate) fn collect_by_extension(
    dir: &Path,
    extension: &str,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_by_extension(&path, extension, out)?;
        } else if path.extension().and_then(OsStr::to_str) == Some(extension) {
            out.push(path);
        }
    }
    Ok(())
}
