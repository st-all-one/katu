//! Testes de identidade/vinculação de sessões (ADR 0008).

use std::path::Path;

use super::{
    AUDIT_EXCLUDE_LINE, SessionId, create, discover_root, ensure_audit_excluded, list, meta_path,
    session_dir,
};
use crate::ports::{Fs, MemFs};

fn repo(fs: &MemFs, root: &Path) {
    fs.write_atomic(&root.join(".git").join("HEAD"), b"ref: refs/heads/main\n")
        .expect("git");
}

#[test]
fn id_is_deterministic_and_parses() {
    let root = Path::new("/work/proj");
    let id = SessionId::new(root, 1_000);
    assert_eq!(id, SessionId::new(root, 1_000));
    assert_ne!(id, SessionId::new(root, 1_001));
    assert_eq!(SessionId::parse(id.as_str()), Some(id));
    assert!(SessionId::parse("x_dead").is_none());
    assert!(SessionId::parse("s_zzzz").is_none());
}

#[test]
fn create_indexes_sessions_in_temporal_order() {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root);
    let late = create(&fs, root, 200, "depois").expect("criar");
    let early = create(&fs, root, 100, "antes").expect("criar");
    let listed = list(&fs, root).expect("listar");
    let ids: Vec<SessionId> = listed.into_iter().map(|meta| meta.id).collect();
    assert_eq!(ids, vec![early.id.clone(), late.id.clone()]);
    assert_ne!(early.id, late.id);
    assert!(fs.exists(&meta_path(&session_dir(root, &early.id))));
}

#[test]
fn audit_is_excluded_locally_and_idempotently() {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root);
    ensure_audit_excluded(&fs, root).expect("excluir");
    ensure_audit_excluded(&fs, root).expect("idempotente");
    let exclude = root.join(".git").join("info").join("exclude");
    let text = String::from_utf8(fs.read(&exclude).expect("ler")).expect("utf8");
    assert_eq!(text.matches(AUDIT_EXCLUDE_LINE).count(), 1);
}

#[test]
fn discover_root_walks_up_to_the_project_marker() {
    let fs = MemFs::new();
    let root = Path::new("/work/proj");
    repo(&fs, root);
    fs.write_atomic(&root.join("src").join("deep").join("f.rs"), b"//")
        .expect("ficheiro");
    assert_eq!(discover_root(&fs, &root.join("src").join("deep")), root);
}
