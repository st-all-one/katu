//! Materialização do worker (E20-T20): script, unidades systemd e lista de subscrições.
//!
//! Vive separado da orquestração para manter cada ficheiro sob o teto de 300 linhas.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use katu_core::error::Error;

use super::Paths;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Script do worker, embutido no binário (sem rede por omissão).
pub(super) const SCRIPT_BODY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/katu-idle.sh"
));

/// Nome do script materializado.
pub(super) const SCRIPT_NAME: &str = "katu-idle.sh";
/// Ficheiro com a lista de projetos subscritos.
pub(super) const WATCHED_NAME: &str = "watched";
/// Unidade systemd do worker.
pub(super) const SERVICE_NAME: &str = "katu-drain.service";
/// Timer systemd do worker.
pub(super) const TIMER_NAME: &str = "katu-drain.timer";

/// Timer systemd do worker (corre a cada 30 min; sobrevive a reinícios).
pub(super) const TIMER_UNIT: &str = "[Unit]\nDescription=katu — timer de auto-drain (worker ocioso)\n\n\
[Timer]\nOnBootSec=5min\nOnUnitActiveSec=30min\nPersistent=true\n\n\
[Install]\nWantedBy=timers.target\n";

/// Materializa o script do worker com permissão de execução.
pub(super) fn write_script(path: &Path) -> Result<(), Error> {
    write_file(path, SCRIPT_BODY)?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|err| Error::io(path.display().to_string(), err))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|err| Error::io(path.display().to_string(), err))?;
    }
    Ok(())
}

/// Escreve `body` em `path`, criando o diretório pai.
pub(super) fn write_file(path: &Path, body: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::io(parent.display().to_string(), err))?;
    }
    fs::write(path, body).map_err(|err| Error::io(path.display().to_string(), err))
}

/// Remove um ficheiro, ignorando a ausência.
pub(super) fn remove_file(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(Error::io(path.display().to_string(), err)),
    }
}

/// Lê a lista de projetos subscritos (ordem preservada, sem linhas vazias).
pub(super) fn read_watched(paths: &Paths) -> Result<Vec<String>, Error> {
    let path = paths.watched();
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(Error::io(path.display().to_string(), err)),
    }
}

/// Grava a lista de projetos subscritos (uma por linha).
pub(super) fn write_watched(paths: &Paths, watched: &[String]) -> Result<(), Error> {
    let mut body = watched.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    write_file(&paths.watched(), &body)
}

/// Unidade systemd do worker, com o caminho real do script.
pub(super) fn service_unit(script: &Path) -> String {
    format!(
        "[Unit]\nDescription=katu — auto-drain de embeddings (worker ocioso)\n\n\
         [Service]\nType=oneshot\nExecStart={}\n",
        script.display()
    )
}
