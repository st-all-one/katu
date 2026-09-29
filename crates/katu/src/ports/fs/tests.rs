//! Testes do adaptador `StdFs` (E07-T04): escrita atómica endurecida.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::{TEMP_COUNTER, temp_path, write_atomic_via, write_bytes_atomic};
use katu_core::ports::FsError;

fn unique_dir() -> Result<PathBuf, FsError> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("katu-test-{pid}-{counter}"));
    fs::create_dir_all(&dir).map_err(|err| FsError::from_io(&err))?;
    Ok(dir)
}

fn read(path: &Path) -> Result<Vec<u8>, FsError> {
    fs::read(path).map_err(|err| FsError::from_io(&err))
}

#[test]
fn atomic_write_creates_the_file() -> Result<(), FsError> {
    let dir = unique_dir()?;
    let target = dir.join("data.txt");
    write_bytes_atomic(&target, b"hello")?;
    assert_eq!(read(&target)?, b"hello".to_vec());
    Ok(())
}

#[test]
fn atomic_write_overwrites_atomically() -> Result<(), FsError> {
    let dir = unique_dir()?;
    let target = dir.join("data.txt");
    write_bytes_atomic(&target, b"first")?;
    write_bytes_atomic(&target, b"second")?;
    assert_eq!(read(&target)?, b"second".to_vec());
    Ok(())
}

#[test]
fn temp_names_are_unique_and_not_the_target() {
    let base = Path::new("/work/a.txt");
    let first = temp_path(base);
    let second = temp_path(base);
    assert_ne!(first, second);
    assert_ne!(first, PathBuf::from(base));
}

#[cfg(unix)]
#[test]
fn atomic_write_refuses_a_planted_symlink() -> Result<(), FsError> {
    let dir = unique_dir()?;
    let target = dir.join("data.txt");
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"original").map_err(|err| FsError::from_io(&err))?;
    let temporary = dir.join("data.txt.tmp");
    std::os::unix::fs::symlink(&victim, &temporary).map_err(|err| FsError::from_io(&err))?;
    let result = write_atomic_via(&target, &temporary, b"attacker");
    assert!(result.is_err(), "O_EXCL devia recusar o symlink plantado");
    assert_eq!(read(&victim)?, b"original".to_vec());
    assert!(!target.exists());
    Ok(())
}
