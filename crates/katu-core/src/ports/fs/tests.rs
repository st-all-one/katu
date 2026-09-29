use super::{Fs, FsError, MemFs};
use std::path::{Path, PathBuf};

#[test]
fn memfs_roundtrip() -> Result<(), FsError> {
    let fs = MemFs::new();
    let path = PathBuf::from("/a.txt");
    fs.write_atomic(&path, b"hi")?;
    assert!(fs.exists(&path));
    assert_eq!(fs.read(&path)?, b"hi".to_vec());
    Ok(())
}

#[test]
fn memfs_missing_is_not_found() {
    let fs = MemFs::new();
    assert_eq!(fs.read(Path::new("/nope")), Err(FsError::NotFound));
}

#[test]
fn memfs_rename_moves_content() -> Result<(), FsError> {
    let fs = MemFs::new();
    let from = Path::new("/a");
    let to = Path::new("/b");
    fs.write_atomic(from, b"hi")?;
    fs.rename(from, to)?;
    assert!(!fs.exists(from));
    assert_eq!(fs.read(to)?, b"hi".to_vec());
    Ok(())
}

#[test]
fn memfs_rename_missing_is_not_found() {
    let fs = MemFs::new();
    assert_eq!(
        fs.rename(Path::new("/nope"), Path::new("/dest")),
        Err(FsError::NotFound)
    );
}

#[test]
fn memfs_lists_in_canonical_order() -> Result<(), FsError> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/dir/b"), b"b")?;
    fs.write_atomic(Path::new("/dir/a"), b"a")?;
    let entries = fs.list_dir(Path::new("/dir"))?;
    assert_eq!(
        entries,
        vec![PathBuf::from("/dir/a"), PathBuf::from("/dir/b")]
    );
    Ok(())
}

#[test]
fn memfs_mtime_advances_monotonically() -> Result<(), FsError> {
    let fs = MemFs::new();
    let first = Path::new("/first");
    let second = Path::new("/second");
    fs.write_atomic(first, b"1")?;
    fs.write_atomic(second, b"2")?;
    assert!(fs.mtime(second)? > fs.mtime(first)?);
    Ok(())
}

#[test]
fn memfs_cas_writes_only_on_match() -> Result<(), FsError> {
    let fs = MemFs::new();
    let path = Path::new("/f");
    fs.write_atomic(path, b"one")?;
    fs.write_atomic_if(path, b"two", b"one")?;
    assert_eq!(fs.read(path)?, b"two".to_vec());
    Ok(())
}

#[test]
fn memfs_cas_refuses_stale_and_keeps_content() -> Result<(), FsError> {
    let fs = MemFs::new();
    let path = Path::new("/f");
    fs.write_atomic(path, b"one")?;
    assert_eq!(
        fs.write_atomic_if(path, b"two", b"OLD"),
        Err(FsError::Stale)
    );
    assert_eq!(fs.read(path)?, b"one".to_vec());
    Ok(())
}

#[test]
fn memfs_cas_refuses_missing() {
    let fs = MemFs::new();
    assert_eq!(
        fs.write_atomic_if(Path::new("/missing"), b"x", b""),
        Err(FsError::Stale)
    );
}
