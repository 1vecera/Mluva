//! Atomic owner-only writes for settings and recovery content.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::{Component, PathBuf};

/// Resolve links and parent components even when the final recovery file no longer exists.
pub(crate) fn resolve_path(path: &Path) -> io::Result<PathBuf> {
    fn resolve(path: &Path, links: usize) -> io::Result<PathBuf> {
        if links == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Too many symbolic links",
            ));
        }
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut result = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::ParentDir => {
                    result.pop();
                }
                Component::CurDir => {}
                _ => {
                    result.push(component.as_os_str());
                    match fs::symlink_metadata(&result) {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            let target = fs::read_link(&result)?;
                            result = resolve(
                                &result.parent().unwrap_or(Path::new("/")).join(target),
                                links - 1,
                            )?;
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Ok(result)
    }
    resolve(path, 40)
}

/// Write a private sibling before replacing the destination, leaving the old document intact on failure.
pub fn atomic_write_private(path: &Path, content: &[u8]) -> io::Result<()> {
    atomic_write(path, content, false)
}

pub fn atomic_write_private_durable(path: &Path, content: &[u8]) -> io::Result<()> {
    atomic_write(path, content, true)
}

fn atomic_write(path: &Path, content: &[u8], durable: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Document needs a filename"))?;
    let prefix = format!(".{}.", filename.to_string_lossy());
    let mut temporary = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary.write_all(content)?;
    if durable {
        temporary.as_file().sync_all()?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_replace_erases_only_the_new_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("document");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("preserved"), b"original").unwrap();
        assert!(atomic_write_private(&destination, b"sensitive temporary text").is_err());
        assert_eq!(
            fs::read(destination.join("preserved")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn an_unrelated_sibling_temporary_file_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("config.json");
        let sibling = directory.path().join(".config.json.existing.tmp");
        fs::write(&sibling, b"other document").unwrap();
        atomic_write_private(&destination, b"complete new document").unwrap();
        assert_eq!(fs::read(&sibling).unwrap(), b"other document");
        assert_eq!(fs::read(destination).unwrap(), b"complete new document");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
