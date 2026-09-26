//! Capacity of the filesystem holding the recordings.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskSpace {
    pub total: u64,
    pub used: u64,
    /// Free for unprivileged writers (what the server can actually use).
    pub free: u64,
}

pub fn space(path: &Path) -> std::io::Result<DiskSpace> {
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| std::io::Error::other("path contains a NUL byte"))?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a valid NUL-terminated string and `s` is a
    // writable statvfs buffer; statvfs only writes into it.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut s) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let unit = s.f_frsize as u64;
    let total = s.f_blocks as u64 * unit;
    let all_free = s.f_bfree as u64 * unit;
    Ok(DiskSpace { total, used: total.saturating_sub(all_free), free: s.f_bavail as u64 * unit })
}

/// Whether this process may write in `dir` (a permission check, no file is
/// written: cheap enough for a status poll).
pub fn writable(dir: &Path) -> bool {
    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: `c_path` is a valid NUL-terminated string.
    dir.is_dir() && unsafe { libc::access(c_path.as_ptr(), libc::W_OK) } == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_current_filesystem() {
        let s = space(Path::new(".")).unwrap();
        assert!(s.total > 0 && s.used <= s.total && s.free <= s.total);
    }

    #[test]
    fn missing_paths_are_errors() {
        assert!(space(Path::new("/definitely/not/here")).is_err());
    }

    #[test]
    fn writable_checks_permission_without_writing() {
        assert!(writable(&std::env::temp_dir()));
        assert!(!writable(Path::new("/proc")), "a read-only place");
        assert!(!writable(Path::new("/definitely/not/here")));
    }
}
