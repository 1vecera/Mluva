//! Move each replaced object aside on its own filesystem. Restore the complete
//! public installation after failure, without deleting a concurrent owner's work.
use crate::{install_paths::Result, install_process::checkpoint};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(PartialEq, Eq)]
struct Stamp(u64, u64, u32, u64, i64, i64);
pub struct Snapshot(BTreeMap<PathBuf, Stamp>);
impl Snapshot {
    pub fn read(path: &Path) -> Result<Self> {
        fn walk(
            path: &Path,
            relative: &Path,
            entries: &mut BTreeMap<PathBuf, Stamp>,
        ) -> Result<()> {
            let meta = match path.symlink_metadata() {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            entries.insert(
                relative.to_owned(),
                Stamp(
                    meta.dev(),
                    meta.ino(),
                    meta.mode(),
                    meta.len(),
                    meta.mtime(),
                    meta.mtime_nsec(),
                ),
            );
            if meta.is_dir() {
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    walk(&entry.path(), &relative.join(entry.file_name()), entries)?;
                }
            }
            Ok(())
        }
        let mut entries = BTreeMap::new();
        walk(path, Path::new(""), &mut entries)?;
        Ok(Self(entries))
    }
    fn unchanged(&self, path: &Path) -> Result<bool> {
        Ok(self.0 == Self::read(path)?.0)
    }
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    let from = CString::new(from.as_os_str().as_encoded_bytes())?;
    let to = CString::new(to.as_os_str().as_encoded_bytes())?;
    if unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn remove(path: &Path) -> Result<()> {
    if path.symlink_metadata()?.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

struct Change {
    target: PathBuf,
    temporary: Option<tempfile::TempDir>,
    previous: bool,
    published: Option<Snapshot>,
}
struct Directory {
    path: PathBuf,
    identity: (u64, u64),
    previous_mode: Option<u32>,
    installed_mode: u32,
}
#[derive(Default)]
pub struct Transaction {
    changes: Vec<Change>,
    directories: Vec<Directory>,
    committed: bool,
}

impl Transaction {
    pub fn directory(&mut self, path: &Path, mode: u32, enforce_mode: bool) -> Result<()> {
        if self.directories.iter().any(|entry| entry.path == path) {
            return Ok(());
        }
        match path.symlink_metadata() {
            Ok(meta) => {
                if !meta.is_dir() {
                    return Err(format!(
                        "Expected a real installation directory: {}",
                        path.display()
                    )
                    .into());
                }
                if enforce_mode && meta.mode() & 0o7777 != mode {
                    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
                    self.directories.push(Directory {
                        path: path.to_owned(),
                        identity: (meta.dev(), meta.ino()),
                        previous_mode: Some(meta.mode() & 0o7777),
                        installed_mode: mode,
                    });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.directory(
                    path.parent().ok_or("Missing installation parent")?,
                    0o755,
                    false,
                )?;
                fs::create_dir(path)?;
                let meta = fs::symlink_metadata(path)?;
                self.directories.push(Directory {
                    path: path.to_owned(),
                    identity: (meta.dev(), meta.ino()),
                    previous_mode: None,
                    installed_mode: mode,
                });
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub fn prepare(&mut self, path: &Path) -> Result<PathBuf> {
        let temporary = tempfile::Builder::new()
            .prefix(".mluva-install-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(path.parent().ok_or("Missing installation parent")?)?;
        let candidate = temporary.path().join("new");
        self.changes.push(Change {
            target: path.to_owned(),
            temporary: Some(temporary),
            previous: false,
            published: None,
        });
        Ok(candidate)
    }

    pub fn publish(&mut self, path: &Path, expected: &Snapshot) -> Result<()> {
        checkpoint()?;
        let change = self
            .changes
            .iter_mut()
            .find(|change| change.target == path)
            .ok_or("Unprepared installation path")?;
        if !expected.unchanged(path)? {
            return Err(format!(
                "Installation path changed while preparing: {}",
                path.display()
            )
            .into());
        }
        let temporary = change
            .temporary
            .as_ref()
            .ok_or("Missing installation backup")?
            .path();
        if !expected.0.is_empty() {
            rename(path, &temporary.join("previous"))?;
            change.previous = true;
            if !expected.unchanged(&temporary.join("previous"))? {
                return Err(format!(
                    "Installation path changed during replacement: {}",
                    path.display()
                )
                .into());
            }
        }
        // Capture identity before rename, so every successful publication is
        // journalled even if the destination becomes unreadable immediately.
        let published = Snapshot::read(&temporary.join("new"))?;
        rename(&temporary.join("new"), path)?;
        change.published = Some(published);
        Ok(())
    }

    pub fn commit(mut self) -> Result<()> {
        checkpoint()?;
        self.committed = true;
        for change in &mut self.changes {
            if let Some(temporary) = change.temporary.take() {
                let path = temporary.path().to_owned();
                if temporary.close().is_err() {
                    eprintln!(
                        "Installation succeeded; a private backup remains at {}.",
                        path.display()
                    );
                }
            }
        }
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for change in self.changes.iter_mut().rev() {
            let Some(temporary) = change.temporary.take() else {
                continue;
            };
            let restored = (|| -> Result<()> {
                if let Some(published) = &change.published {
                    if !published.unchanged(&change.target)? {
                        return Err("The published path changed".into());
                    }
                    remove(&change.target)?;
                }
                if change.previous {
                    rename(&temporary.path().join("previous"), &change.target)?;
                }
                Ok(())
            })();
            if restored.is_err() {
                let backup = temporary.keep();
                eprintln!(
                    "Preserved a changed installation path at {}. Recovery files remain at {}.",
                    change.target.display(),
                    backup.display()
                );
            }
        }
        for directory in self.directories.iter().rev() {
            if let Ok(meta) = directory.path.symlink_metadata()
                && meta.is_dir()
                && (meta.dev(), meta.ino()) == directory.identity
                && meta.mode() & 0o7777 == directory.installed_mode
            {
                if let Some(mode) = directory.previous_mode {
                    let _ = fs::set_permissions(&directory.path, fs::Permissions::from_mode(mode));
                } else {
                    let _ = fs::remove_dir(&directory.path);
                }
            }
        }
    }
}
