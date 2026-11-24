use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;

fn work_with_session(root: &Path) -> (WorkDir, Session) {
    let work = WorkDir::new(root).unwrap();
    work.initialize().unwrap();
    let session = Session::new();
    save_session(&session, work.root()).unwrap();
    (work, session)
}

#[test]
fn an_oversize_session_file_is_skipped() {
    let root = tempdir().unwrap();
    let (work, session) = work_with_session(root.path());
    let sessions_dir = work.sessions_dir();
    let mut content = fs::read_to_string(sessions_dir.join(format!("{}.md", session.id))).unwrap();
    content.push('\n');
    content.push_str(&"x".repeat(MAX_SESSION_FILE_BYTES + 1));
    fs::write(sessions_dir.join("big.md"), content).unwrap();

    let sessions = load_all_sessions(&work).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);
}

#[test]
fn a_fifo_named_like_a_session_does_not_block_the_load() {
    let root = tempdir().unwrap();
    let (work, session) = work_with_session(root.path());
    let path = work.sessions_dir().join("pipe.md");
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(load_all_sessions(&work)).unwrap());
    let sessions = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("session loading did not return")
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);
}
