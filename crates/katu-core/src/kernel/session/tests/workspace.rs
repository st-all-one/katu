use crate::kernel::session::Session;
use crate::ports::MemFs;
use katu_policy::ResolvedPath;
use std::path::Path;

#[test]
fn set_workspace_survives_replay() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions-ws");
    let root = ResolvedPath::from_canonical("/work")?;
    let mut session = Session::open(&fs, dir)?;
    session.set_workspace(&root)?;
    session.verify()?;
    assert_eq!(session.state().workspace.as_ref(), Some(&root));
    let reopened = Session::open(&fs, dir)?;
    assert_eq!(reopened.state().workspace.as_ref(), Some(&root));
    Ok(())
}
