use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub struct Publications(Vec<Target>);
struct Target {
    path: PathBuf,
    directory: File,
    leaf: CString,
    ancestors: Vec<(u64, u64)>,
}

fn identity(file: &File) -> Result<(u64, u64), String> {
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    Ok((metadata.dev(), metadata.ino()))
}

fn open_relative(
    directory: &File,
    name: &CString,
    flags: i32,
    mode: libc::mode_t,
) -> std::io::Result<File> {
    // SAFETY: directory owns a live descriptor, name is terminated, and successful
    // openat returns a new descriptor transferred exactly once into File.
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn directory(path: &Path) -> Result<(File, Vec<(u64, u64)>), String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut current = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|e| e.to_string())?;
    let mut ancestors = vec![identity(&current)?];
    for component in absolute.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                let name = CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
                current = open_relative(&current, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
                    .map_err(|e| e.to_string())?;
                ancestors.push(identity(&current)?);
            }
            _ => return Err("output directory traversal is not admitted".into()),
        }
    }
    Ok((current, ancestors))
}

impl Publications {
    pub fn admit(outputs: &[(&str, &Path)], session: Option<&Path>) -> Result<Self, String> {
        let protected = session
            .map(|path| directory(path).and_then(|(file, _)| identity(&file)))
            .transpose()?;
        let mut targets: Vec<Target> = Vec::with_capacity(outputs.len());
        for (_, path) in outputs {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let leaf = path.file_name().ok_or("output requires a file name")?;
            let (directory, ancestors) = directory(parent)?;
            if protected.is_some_and(|id| ancestors.contains(&id)) {
                return Err("output must not modify exact session artifacts".into());
            }
            let leaf = CString::new(leaf.as_bytes()).map_err(|e| e.to_string())?;
            let parent_id = identity(&directory)?;
            if targets
                .iter()
                .any(|target| target.ancestors.last() == Some(&parent_id) && target.leaf == leaf)
            {
                return Err("output paths resolve to the same directory entry".into());
            }
            let target = Target {
                path: path.to_path_buf(),
                directory,
                leaf,
                ancestors,
            };
            target.validate_leaf()?;
            targets.push(target);
        }
        Ok(Self(targets))
    }

    pub fn json<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
        limit: u64,
    ) -> Result<(), String> {
        let limit = usize::try_from(limit).map_err(|_| "JSON output bound is not addressable")?;
        let mut writer = super::BoundedJsonOutput {
            bytes: Vec::new(),
            limit,
        };
        serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
        self.bytes(path, &writer.bytes)
    }

    pub fn bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.0
            .iter()
            .find(|target| target.path == path)
            .ok_or("output was not admitted")?
            .publish(bytes)
    }
}

impl Target {
    fn validate_leaf(&self) -> Result<(), String> {
        match open_relative(&self.directory, &self.leaf, libc::O_RDONLY, 0) {
            Ok(file) if file.metadata().map_err(|e| e.to_string())?.is_file() => Ok(()),
            Ok(_) => Err("output is not a regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn publish(&self, bytes: &[u8]) -> Result<(), String> {
        self.validate_leaf()?;
        for attempt in 0..128 {
            let name = CString::new(format!(
                ".query-events-{}-{attempt}.tmp",
                std::process::id()
            ))
            .map_err(|e| e.to_string())?;
            let mut file = match open_relative(
                &self.directory,
                &name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            ) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            };
            let result = (|| {
                file.write_all(bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|e| e.to_string())?;
                // SAFETY: both names and the retained directory descriptor are live.
                if unsafe {
                    libc::renameat(
                        self.directory.as_raw_fd(),
                        name.as_ptr(),
                        self.directory.as_raw_fd(),
                        self.leaf.as_ptr(),
                    )
                } < 0
                {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                self.directory.sync_all().map_err(|e| e.to_string())
            })();
            // SAFETY: cleanup is relative to the same retained directory capability.
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0);
            }
            return result;
        }
        Err("output temporary-file collision bound exceeded".into())
    }
}

pub fn resolved_entry(path: &Path) -> Option<PathBuf> {
    if let Ok(path) = path.canonicalize() {
        return Some(path);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Some(parent.canonicalize().ok()?.join(path.file_name()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_keeps_admitted_directory_after_parent_substitution() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("output");
        let moved = root.path().join("moved");
        let session = root.path().join("session");
        std::fs::create_dir(&parent).unwrap();
        std::fs::create_dir(&session).unwrap();
        std::fs::write(session.join("session.json"), b"original").unwrap();
        let path = parent.join("session.json");
        let outputs = Publications::admit(&[("result", &path)], Some(&session)).unwrap();
        std::fs::rename(&parent, &moved).unwrap();
        std::os::unix::fs::symlink(&session, &parent).unwrap();
        outputs.bytes(&path, b"receipt").unwrap();
        assert_eq!(
            std::fs::read(session.join("session.json")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(moved.join("session.json")).unwrap(),
            b"receipt"
        );
    }
    #[test]
    fn parent_alias_and_parent_traversal_are_rejected_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert!(Publications::admit(&[("result", &alias.join("out"))], None).is_err());
        assert!(Publications::admit(&[("result", &real.join("../out"))], None).is_err());
        assert!(Publications::admit(
            &[
                ("result", &real.join("out")),
                ("receipt", &real.join("./out"))
            ],
            None
        )
        .is_err());
    }
}
