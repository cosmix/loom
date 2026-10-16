use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

// Apple's WebKit SandboxSPI.h declares the variadic int ABI and PATH = 1.
// libsandbox exports both this function and the NO_REPORT flag (a variable,
// not an enum literal). Never treat an error (-1) as a denial or a sandbox.
#[link(name = "sandbox")]
unsafe extern "C" {
    fn sandbox_check(pid: libc::pid_t, operation: *const libc::c_char, filter: i32, ...) -> i32;
    static SANDBOX_CHECK_NO_REPORT: i32;
}

pub(super) fn sandboxed(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: a null operation with filter NONE queries sandbox membership;
    // this form consumes no variadic argument.
    unsafe { sandbox_check(pid, std::ptr::null(), 0) > 0 }
}

pub(super) fn protected(pid: u32, path: &Path) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    [
        c"file-write-data",
        c"file-write-create",
        c"file-write-unlink",
    ]
    .iter()
    .all(|operation| {
        // SAFETY: both strings remain live throughout the call. PATH
        // consumes one NUL-terminated pathname; the flag is read-only.
        unsafe {
            sandbox_check(
                pid,
                operation.as_ptr(),
                1 | SANDBOX_CHECK_NO_REPORT,
                path.as_ptr(),
            ) > 0
        }
    })
}
