//! Resolve Linux executables using the caller's actual access rights.
use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub fn find_executable(name: impl AsRef<OsStr>) -> Option<PathBuf> {
    let name = Path::new(name.as_ref());
    if name
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return accessible(name).then(|| name.into());
    }
    let search_path = env::var_os("PATH").unwrap_or_else(|| {
        let length = unsafe { libc::confstr(libc::_CS_PATH, std::ptr::null_mut(), 0) };
        if length == 0 {
            return OsString::from("/bin:/usr/bin");
        }
        let mut value = vec![0; length];
        unsafe { libc::confstr(libc::_CS_PATH, value.as_mut_ptr().cast(), length) };
        value.truncate(length - 1);
        OsString::from_vec(value)
    });
    env::split_paths(&search_path)
        .map(|directory| directory.join(name))
        .find(|path| accessible(path))
}
fn accessible(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && CString::new(path.as_os_str().as_bytes())
                .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
    })
}
