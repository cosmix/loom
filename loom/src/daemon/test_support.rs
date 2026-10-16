//! Owned socket fixtures shared by daemon unit tests.

pub(super) fn stale_socket(path: &std::path::Path) {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileTypeExt;

    // Bind without listening: a concurrent fork can inherit the fd, but
    // cannot keep a listener alive after this fixture drops its copy.
    // SAFETY: socket has no pointer arguments and returns an owned fd.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    assert!(raw >= 0, "socket: {}", std::io::Error::last_os_error());
    // SAFETY: raw is a valid, uniquely owned socket descriptor.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: zero initializes every field, including the path terminator.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as _;
    let bytes = path.as_os_str().as_bytes();
    assert!(bytes.len() < address.sun_path.len());
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as libc::c_char;
    }
    #[cfg(target_os = "macos")]
    {
        address.sun_len = std::mem::size_of_val(&address) as _;
    }
    // SAFETY: the live fd and initialized address remain valid for bind.
    let result = unsafe {
        libc::bind(
            socket.as_raw_fd(),
            (&raw const address).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    assert_eq!(result, 0, "bind: {}", std::io::Error::last_os_error());
    drop(socket);
    assert!(std::fs::symlink_metadata(path)
        .unwrap()
        .file_type()
        .is_socket());
}
