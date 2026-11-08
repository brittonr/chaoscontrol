//! Persisted replay parent snapshot artifact references and stores.
//!
//! r[impl chaoscontrol.replay_debugger.shared_store]
//!
//! The public evidence boundary is a small JSON reference. Snapshot bytes stay
//! Rust-derived runtime artifacts; Nickel/checkers validate refs, digests, and
//! bounded paths rather than owning VM internals. Filesystem operations pin
//! no-follow directory capabilities before encoding, reading, or collecting;
//! renaming or symlinking an ancestor cannot redirect an in-flight operation.

use chaoscontrol_vmm::controller::SimulationSnapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snafu::Snafu;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use chaoscontrol_replay_evidence_core::dto::ReplayParentSnapshotRef;
use chaoscontrol_replay_evidence_core::validate as core_validate;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = core_validate::CURRENT_SNAPSHOT_SCHEMA_VERSION;
pub const SNAPSHOT_CODEC: &str = core_validate::CURRENT_SNAPSHOT_CODEC;
pub const FILE_STORE_KIND: &str = core_validate::FILE_STORE_KIND;
pub const SNAPSHOT_DIR: &str = "snapshots";
const BYTES_PER_KIB: u64 = 1024;
const KIB_PER_MIB: u64 = 1024;
const BYTES_PER_MIB: u64 = BYTES_PER_KIB * KIB_PER_MIB;
const MAX_COMPRESSED_SNAPSHOT_BYTES: u64 = 256 * BYTES_PER_MIB;
pub const MAX_DECOMPRESSED_SNAPSHOT_BYTES: u64 = 2048 * BYTES_PER_MIB;
const MAX_SNAPSHOT_DIRECTORY_ENTRIES: usize = 4_096;
const READ_LIMIT_SENTINEL_BYTES: u64 = 1;
const SNAPSHOT_COMPRESSION_LEVEL: i32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotArtifactEnvelope {
    pub schema_version: u32,
    pub codec: String,
    pub replay_parent_depth: u32,
    pub tick: u64,
    pub vm_count: usize,
    pub snapshot: SimulationSnapshot,
}

#[derive(Debug, Snafu)]
pub enum SnapshotStoreError {
    #[snafu(display("snapshot ref uses unsupported store '{store}'"))]
    UnsupportedStore { store: String },
    #[snafu(display("snapshot ref uses unsupported codec '{codec}'"))]
    UnsupportedCodec { codec: String },
    #[snafu(display("snapshot ref uses unsupported schema version {version}"))]
    UnsupportedSchema { version: u32 },
    #[snafu(display("snapshot path escapes store root: {path}"))]
    PathEscape { path: String },
    #[snafu(display("snapshot ref digest is malformed: {digest}"))]
    InvalidDigest { digest: String },
    #[snafu(display("snapshot artifact missing: {path}"))]
    Missing { path: String },
    #[snafu(display("snapshot artifact is not a regular file: {path}"))]
    NotRegular { path: String },
    #[snafu(display("snapshot directory is not a plain directory: {path}"))]
    NotDirectory { path: String },
    #[snafu(display("snapshot artifact exceeds {limit} bytes: {path}"))]
    TooLarge { path: String, limit: u64 },
    #[snafu(display("snapshot decompression exceeds {limit} bytes"))]
    DecompressedTooLarge { limit: u64 },
    #[snafu(display("snapshot artifact metadata mismatch: {field}"))]
    MetadataMismatch { field: &'static str },
    #[snafu(display(
        "snapshot artifact digest mismatch for {path}: expected {expected}, got {actual}"
    ))]
    DigestMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[snafu(display("snapshot artifact I/O error: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("snapshot artifact JSON error: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("snapshot artifact CBOR encode error: {source}"))]
    CborEncode {
        source: ciborium::ser::Error<std::io::Error>,
    },
    #[snafu(display("snapshot artifact CBOR decode error: {source}"))]
    CborDecode {
        source: ciborium::de::Error<std::io::Error>,
    },
}

#[cfg(test)]
fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, SnapshotStoreError> {
    encode_cbor_with_limit(value, MAX_DECOMPRESSED_SNAPSHOT_BYTES)
}

fn encode_cbor_with_limit<T: Serialize>(
    value: &T,
    maximum_bytes: u64,
) -> Result<Vec<u8>, SnapshotStoreError> {
    let mut output = BoundedSnapshotBytes {
        bytes: Vec::new(),
        maximum_bytes,
        exceeded: false,
    };
    ciborium::ser::into_writer(value, &mut output).map_err(|source| {
        if output.exceeded {
            SnapshotStoreError::DecompressedTooLarge {
                limit: maximum_bytes,
            }
        } else {
            SnapshotStoreError::CborEncode { source }
        }
    })?;
    Ok(output.bytes)
}

struct BoundedSnapshotBytes {
    bytes: Vec<u8>,
    maximum_bytes: u64,
    exceeded: bool,
}
impl Write for BoundedSnapshotBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (self.bytes.len() as u64).saturating_add(bytes.len() as u64) > self.maximum_bytes {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "snapshot CBOR exceeds decompressed byte bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn read_snapshot_bytes(mut file: File, display_path: &str) -> Result<Vec<u8>, SnapshotStoreError> {
    let metadata = file
        .metadata()
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if !metadata.file_type().is_file() {
        return Err(SnapshotStoreError::NotRegular {
            path: display_path.to_string(),
        });
    }
    if metadata.len() > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    let read_limit = MAX_COMPRESSED_SNAPSHOT_BYTES
        .checked_add(READ_LIMIT_SENTINEL_BYTES)
        .ok_or(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        })?;
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(usize::try_from(MAX_COMPRESSED_SNAPSHOT_BYTES).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
    Read::by_ref(&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if bytes.len() as u64 > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(SnapshotStoreError::Io {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "snapshot changed while reading",
            ),
        });
    }
    Ok(bytes)
}

fn decompress_snapshot(bytes: &[u8], maximum_bytes: u64) -> Result<Vec<u8>, SnapshotStoreError> {
    let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(bytes))
        .map_err(|source| SnapshotStoreError::Io { source })?;
    let read_limit = maximum_bytes.checked_add(READ_LIMIT_SENTINEL_BYTES).ok_or(
        SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_bytes,
        },
    )?;
    let mut decompressed = Vec::new();
    decoder
        .take(read_limit)
        .read_to_end(&mut decompressed)
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if decompressed.len() as u64 > maximum_bytes {
        return Err(SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_bytes,
        });
    }
    Ok(decompressed)
}

pub trait SnapshotStore {
    fn put_snapshot(
        &self,
        snapshot: &SimulationSnapshot,
        replay_parent_depth: u32,
    ) -> Result<ReplayParentSnapshotRef, SnapshotStoreError>;

    fn get_snapshot_artifact(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SnapshotArtifactEnvelope, SnapshotStoreError>;

    fn get_snapshot(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SimulationSnapshot, SnapshotStoreError> {
        Ok(self.get_snapshot_artifact(reference)?.snapshot)
    }

    fn has_snapshot(&self, reference: &ReplayParentSnapshotRef) -> bool;

    fn gc_unreferenced(
        &self,
        keep: &[ReplayParentSnapshotRef],
    ) -> Result<usize, SnapshotStoreError>;
}

#[derive(Debug, Clone)]
pub struct FileSnapshotStore {
    root: PathBuf,
}

impl FileSnapshotStore {
    pub fn new(run_output_dir: impl AsRef<Path>) -> Self {
        Self {
            root: run_output_dir.as_ref().to_path_buf(),
        }
    }

    fn snapshots_directory(&self, create: bool) -> Result<File, SnapshotStoreError> {
        let root = open_directory(&self.root, create)?;
        open_child_directory(&root, OsStr::new(SNAPSHOT_DIR), create)
    }

    fn resolve_ref(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<(File, OsString), SnapshotStoreError> {
        validate_ref_shape(reference)?;
        let directory = self
            .snapshots_directory(false)
            .map_err(|error| match error {
                SnapshotStoreError::Io { source }
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    SnapshotStoreError::Missing {
                        path: reference.path.clone(),
                    }
                }
                error => error,
            })?;
        let name = Path::new(&reference.path).file_name().ok_or_else(|| {
            SnapshotStoreError::PathEscape {
                path: reference.path.clone(),
            }
        })?;
        Ok((directory, name.to_os_string()))
    }
}

impl SnapshotStore for FileSnapshotStore {
    fn put_snapshot(
        &self,
        snapshot: &SimulationSnapshot,
        replay_parent_depth: u32,
    ) -> Result<ReplayParentSnapshotRef, SnapshotStoreError> {
        let directory = self.snapshots_directory(true)?;
        put_snapshot_in_directory(&directory, snapshot, replay_parent_depth)
    }

    fn get_snapshot_artifact(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SnapshotArtifactEnvelope, SnapshotStoreError> {
        let (directory, name) = self.resolve_ref(reference)?;
        let file = open_snapshot_file(&directory, &name, &reference.path)?;
        let bytes = read_snapshot_bytes(file, &reference.path)?;
        decode_snapshot_artifact(reference, &bytes)
    }

    fn has_snapshot(&self, reference: &ReplayParentSnapshotRef) -> bool {
        self.get_snapshot_artifact(reference).is_ok()
    }

    fn gc_unreferenced(
        &self,
        keep: &[ReplayParentSnapshotRef],
    ) -> Result<usize, SnapshotStoreError> {
        for reference in keep {
            validate_ref_shape(reference)?;
        }
        let directory = match self.snapshots_directory(false) {
            Ok(directory) => directory,
            Err(SnapshotStoreError::Io { source })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(0)
            }
            Err(error) => return Err(error),
        };
        gc_in_directory(&directory, keep)
    }
}

fn c_name(name: &OsStr) -> Result<CString, SnapshotStoreError> {
    CString::new(name.as_bytes()).map_err(|_| SnapshotStoreError::PathEscape {
        path: name.to_string_lossy().into_owned(),
    })
}

fn open_at(directory: &File, name: &OsStr, flags: i32) -> std::io::Result<File> {
    let name = CString::new(name.as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in snapshot path")
    })?;
    // SAFETY: the directory and NUL-terminated leaf remain live; a successful
    // openat returns a new, uniquely owned descriptor. Never follow a leaf link.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: fd is a newly opened descriptor, owned by this File only.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_child_directory(
    parent: &File,
    name: &OsStr,
    create: bool,
) -> Result<File, SnapshotStoreError> {
    let flags = libc::O_RDONLY | libc::O_DIRECTORY;
    let result = match open_at(parent, name, flags) {
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
            let child = c_name(name)?;
            // SAFETY: live parent capability and NUL-terminated component.
            if unsafe { libc::mkdirat(parent.as_raw_fd(), child.as_ptr(), 0o700) } < 0 {
                let source = std::io::Error::last_os_error();
                if source.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(SnapshotStoreError::Io { source });
                }
            }
            // Reopening never follows a substitute link, including one installed
            // between mkdirat and openat.
            open_at(parent, name, flags)
        }
        result => result,
    };
    result.map_err(|source| {
        if matches!(
            source.raw_os_error(),
            Some(libc::ELOOP) | Some(libc::ENOTDIR)
        ) {
            SnapshotStoreError::NotDirectory {
                path: name.to_string_lossy().into_owned(),
            }
        } else {
            SnapshotStoreError::Io { source }
        }
    })
}

fn open_directory(path: &Path, create: bool) -> Result<File, SnapshotStoreError> {
    let mut directory = File::open(if path.is_absolute() { "/" } else { "." })
        .map_err(|source| SnapshotStoreError::Io { source })?;
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => (),
            // A caller-selected root may contain `..`; walk it through the
            // pinned parent too, rather than resolving any path prefix again.
            Component::Normal(name) => directory = open_child_directory(&directory, name, create)?,
            Component::ParentDir => {
                directory = open_child_directory(&directory, OsStr::new(".."), false)?;
            }
            Component::Prefix(_) => {
                return Err(SnapshotStoreError::PathEscape {
                    path: path.display().to_string(),
                });
            }
        }
    }
    Ok(directory)
}

fn open_snapshot_file(
    directory: &File,
    name: &OsStr,
    display_path: &str,
) -> Result<File, SnapshotStoreError> {
    open_at(directory, name, libc::O_RDONLY).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            SnapshotStoreError::Missing {
                path: display_path.into(),
            }
        } else if source.raw_os_error() == Some(libc::ELOOP) {
            SnapshotStoreError::NotRegular {
                path: display_path.into(),
            }
        } else {
            SnapshotStoreError::Io { source }
        }
    })
}

struct StagedSnapshot<'a> {
    directory: &'a File,
    name: CString,
    file: File,
    published: bool,
}

impl Drop for StagedSnapshot<'_> {
    fn drop(&mut self) {
        if !self.published {
            // SAFETY: cleanup is relative to the same pinned staging directory;
            // unlinkat cannot follow a substituted leaf or ancestor symlink.
            unsafe { libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0) };
        }
    }
}

fn put_snapshot_in_directory(
    directory: &File,
    snapshot: &SimulationSnapshot,
    replay_parent_depth: u32,
) -> Result<ReplayParentSnapshotRef, SnapshotStoreError> {
    let (reference, bytes) =
        encode_snapshot_artifact(snapshot, replay_parent_depth, MAX_COMPRESSED_SNAPSHOT_BYTES)?;
    static NEXT_STAGING_FILE: AtomicU64 = AtomicU64::new(0);
    let staging_name = format!(
        ".snapshot-{}-{}.tmp",
        std::process::id(),
        NEXT_STAGING_FILE.fetch_add(1, Ordering::Relaxed),
    );
    // Exclusive creation fails closed on collisions; no unbounded retry or
    // pathname-based temporary-file API can re-resolve our pinned directory.
    let file = open_at(
        directory,
        OsStr::new(&staging_name),
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
    )
    .map_err(|source| SnapshotStoreError::Io { source })?;
    let mut staged = StagedSnapshot {
        directory,
        name: c_name(OsStr::new(&staging_name))?,
        file,
        published: false,
    };
    staged
        .file
        .write_all(&bytes)
        .map_err(|source| SnapshotStoreError::Io { source })?;
    staged
        .file
        .sync_all()
        .map_err(|source| SnapshotStoreError::Io { source })?;
    let destination = c_name(Path::new(&reference.path).file_name().ok_or_else(|| {
        SnapshotStoreError::PathEscape {
            path: reference.path.clone(),
        }
    })?)?;
    // SAFETY: source and destination are leaf names in the same live directory
    // capability. Replacement is atomic and never follows a destination symlink.
    if unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            staged.name.as_ptr(),
            directory.as_raw_fd(),
            destination.as_ptr(),
        )
    } < 0
    {
        return Err(SnapshotStoreError::Io {
            source: std::io::Error::last_os_error(),
        });
    }
    staged.published = true;
    directory
        .sync_all()
        .map_err(|source| SnapshotStoreError::Io { source })?;
    Ok(reference)
}

fn gc_in_directory(
    directory: &File,
    keep: &[ReplayParentSnapshotRef],
) -> Result<usize, SnapshotStoreError> {
    let keep: std::collections::BTreeSet<&OsStr> = keep
        .iter()
        .filter_map(|reference| Path::new(&reference.path).file_name())
        .collect();
    let mut entries = list_snapshot_entries(directory)?;
    entries.sort();
    // Preserve all-or-nothing preflight for non-files and the entry bound.
    // fstatat checks the leaf itself, never a target behind a link.
    for name in &entries {
        let leaf = c_name(name)?;
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: valid directory, C name, and writable stat output.
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                leaf.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            return Err(SnapshotStoreError::Io {
                source: std::io::Error::last_os_error(),
            });
        }
        // SAFETY: successful fstatat initialized metadata.
        if unsafe { metadata.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(SnapshotStoreError::NotRegular {
                path: format!("{SNAPSHOT_DIR}/{}", name.to_string_lossy()),
            });
        }
    }
    let mut removed = 0;
    for name in entries {
        if !keep.contains(name.as_os_str()) {
            let leaf = c_name(&name)?;
            // SAFETY: only a leaf can be removed from the pinned directory.
            // A concurrent replacement with a symlink removes the link itself,
            // not its target; a replacement directory is rejected by unlinkat.
            if unsafe { libc::unlinkat(directory.as_raw_fd(), leaf.as_ptr(), 0) } < 0 {
                return Err(SnapshotStoreError::Io {
                    source: std::io::Error::last_os_error(),
                });
            }
            removed += 1;
        }
    }
    Ok(removed)
}

/// Encode the current versioned CBOR/zstd envelope and exact store reference.
/// The caller may lower (never raise) the compressed-byte ceiling.
/// No filesystem access occurs, allowing a capability-relative output shell.
pub fn encode_snapshot_artifact(
    snapshot: &SimulationSnapshot,
    replay_parent_depth: u32,
    maximum_bytes: u64,
) -> Result<(ReplayParentSnapshotRef, Vec<u8>), SnapshotStoreError> {
    encode_snapshot_artifact_with_limits(
        snapshot,
        replay_parent_depth,
        maximum_bytes,
        MAX_DECOMPRESSED_SNAPSHOT_BYTES,
    )
    .map(|(reference, bytes, _)| (reference, bytes))
}

/// The file-store encoder with a lower decoded-payload ceiling and exact byte
/// accounting, for a multi-checkpoint session's aggregate retained-state budget.
pub fn encode_snapshot_artifact_with_limits(
    snapshot: &SimulationSnapshot,
    replay_parent_depth: u32,
    maximum_bytes: u64,
    maximum_decoded_bytes: u64,
) -> Result<(ReplayParentSnapshotRef, Vec<u8>, u64), SnapshotStoreError> {
    validate_snapshot_execution(snapshot)?;
    if maximum_decoded_bytes == 0 || maximum_decoded_bytes > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_decoded_bytes,
        });
    }
    if maximum_bytes == 0 || maximum_bytes > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: "snapshot write ceiling".into(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    #[derive(Serialize)]
    struct BorrowedEnvelope<'a> {
        schema_version: u32,
        codec: &'static str,
        replay_parent_depth: u32,
        tick: u64,
        vm_count: usize,
        snapshot: &'a SimulationSnapshot,
    }
    let envelope = BorrowedEnvelope {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        codec: SNAPSHOT_CODEC,
        replay_parent_depth,
        tick: snapshot.tick,
        vm_count: snapshot.vm_snapshots.len(),
        snapshot,
    };
    let uncompressed = encode_cbor_with_limit(&envelope, maximum_decoded_bytes)?;
    let decoded_bytes = uncompressed.len() as u64;
    let mut compressed = BoundedSnapshotBytes {
        bytes: Vec::new(),
        maximum_bytes,
        exceeded: false,
    };
    let compression: Result<(), std::io::Error> = (|| {
        let mut encoder =
            zstd::stream::write::Encoder::new(&mut compressed, SNAPSHOT_COMPRESSION_LEVEL)?;
        encoder.write_all(&uncompressed)?;
        encoder.finish()?;
        Ok(())
    })();
    if compressed.exceeded {
        return Err(SnapshotStoreError::TooLarge {
            path: "generated snapshot".into(),
            limit: maximum_bytes,
        });
    }
    compression.map_err(|source| SnapshotStoreError::Io { source })?;
    let bytes = compressed.bytes;
    let digest = digest_bytes(&bytes);
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    let path = format!("{SNAPSHOT_DIR}/{hex}.snapshot.bin");
    Ok((
        ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.into(),
            digest,
            codec: SNAPSHOT_CODEC.into(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path,
        },
        bytes,
        decoded_bytes,
    ))
}

/// Decode retained bytes using the same bounded codec and identity checks as
/// the file store. Capability-relative callers can pin their directory and file
/// handles before passing bytes; this function performs no filesystem access.
pub fn decode_snapshot_artifact(
    reference: &ReplayParentSnapshotRef,
    bytes: &[u8],
) -> Result<SnapshotArtifactEnvelope, SnapshotStoreError> {
    decode_snapshot_artifact_with_limit(reference, bytes, MAX_DECOMPRESSED_SNAPSHOT_BYTES)
        .map(|(artifact, _)| artifact)
}

/// Decode with a lower, explicitly accounted decompressed-payload ceiling.
pub fn decode_snapshot_artifact_with_limit(
    reference: &ReplayParentSnapshotRef,
    bytes: &[u8],
    maximum_decoded_bytes: u64,
) -> Result<(SnapshotArtifactEnvelope, u64), SnapshotStoreError> {
    if maximum_decoded_bytes == 0 || maximum_decoded_bytes > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_decoded_bytes,
        });
    }
    validate_ref_shape(reference)?;
    if bytes.len() as u64 > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: reference.path.clone(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    let actual = digest_bytes(bytes);
    if actual != reference.digest {
        return Err(SnapshotStoreError::DigestMismatch {
            path: reference.path.clone(),
            expected: reference.digest.clone(),
            actual,
        });
    }
    let decompressed = decompress_snapshot(bytes, maximum_decoded_bytes)?;
    let mut cursor = std::io::Cursor::new(decompressed.as_slice());
    let artifact: SnapshotArtifactEnvelope = ciborium::de::from_reader(&mut cursor)
        .map_err(|source| SnapshotStoreError::CborDecode { source })?;
    if cursor.position() != decompressed.len() as u64 {
        return Err(SnapshotStoreError::MetadataMismatch {
            field: "trailing_bytes",
        });
    }
    if artifact.codec != reference.codec {
        return Err(SnapshotStoreError::MetadataMismatch { field: "codec" });
    }
    if artifact.schema_version != reference.schema_version {
        return Err(SnapshotStoreError::MetadataMismatch {
            field: "schema_version",
        });
    }
    if artifact.tick != artifact.snapshot.tick {
        return Err(SnapshotStoreError::MetadataMismatch { field: "tick" });
    }
    if artifact.vm_count != artifact.snapshot.vm_snapshots.len() {
        return Err(SnapshotStoreError::MetadataMismatch { field: "vm_count" });
    }
    validate_snapshot_execution(&artifact.snapshot)?;
    Ok((artifact, decompressed.len() as u64))
}

fn validate_snapshot_execution(snapshot: &SimulationSnapshot) -> Result<(), SnapshotStoreError> {
    for (vm, _) in &snapshot.vm_snapshots {
        vm.validate_retained_boundary()
            .map_err(|_| SnapshotStoreError::MetadataMismatch {
                field: "complete_settled_execution_state",
            })?;
    }
    Ok(())
}

fn list_snapshot_entries(directory: &File) -> Result<Vec<OsString>, SnapshotStoreError> {
    // Open "." relative to the capability for an independent directory offset.
    // dup/try_clone would share offsets and make repeated scans incomplete.
    let scan = open_at(
        directory,
        OsStr::new("."),
        libc::O_RDONLY | libc::O_DIRECTORY,
    )
    .map_err(|source| SnapshotStoreError::Io { source })?;
    let fd = scan.into_raw_fd();
    // SAFETY: fd is an owned directory descriptor; fdopendir owns it on success.
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        let source = std::io::Error::last_os_error();
        // SAFETY: fdopendir failed, so ownership of fd was not transferred.
        unsafe { libc::close(fd) };
        return Err(SnapshotStoreError::Io { source });
    }
    struct DirectoryStream(*mut libc::DIR);
    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the successful fdopendir.
            unsafe { libc::closedir(self.0) };
        }
    }
    let stream = DirectoryStream(stream);
    let mut entries = Vec::new();
    loop {
        // SAFETY: errno is thread-local and stream is live and exclusively used.
        unsafe { *libc::__errno_location() = 0 };
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let source = std::io::Error::last_os_error();
            if source.raw_os_error() != Some(0) {
                return Err(SnapshotStoreError::Io { source });
            }
            break;
        }
        // SAFETY: readdir returned a live entry with a NUL-terminated d_name;
        // copy it before the next readdir can overwrite that storage.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if entries.len() >= MAX_SNAPSHOT_DIRECTORY_ENTRIES {
            return Err(SnapshotStoreError::Io {
                source: std::io::Error::other("snapshot directory entry bound exceeded"),
            });
        }
        entries.push(OsStr::from_bytes(name).to_os_string());
    }
    Ok(entries)
}

pub fn validate_ref_shape(reference: &ReplayParentSnapshotRef) -> Result<(), SnapshotStoreError> {
    // Admissibility decisions live in the shared core; this shell maps them
    // onto the store error variants for stable diagnostics.
    if reference.store != core_validate::FILE_STORE_KIND {
        return Err(SnapshotStoreError::UnsupportedStore {
            store: reference.store.clone(),
        });
    }
    if reference.codec != core_validate::CURRENT_SNAPSHOT_CODEC {
        return Err(SnapshotStoreError::UnsupportedCodec {
            codec: reference.codec.clone(),
        });
    }
    if reference.schema_version != core_validate::CURRENT_SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotStoreError::UnsupportedSchema {
            version: reference.schema_version,
        });
    }
    let rel = Path::new(&reference.path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || !reference.path.starts_with("snapshots/")
        || rel.components().count() != 2
    {
        return Err(SnapshotStoreError::PathEscape {
            path: reference.path.clone(),
        });
    }
    if !is_sha256_digest(&reference.digest) {
        return Err(SnapshotStoreError::InvalidDigest {
            digest: reference.digest.clone(),
        });
    }
    Ok(())
}

fn is_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

/// Optional host-side redb store/index seam. It is intentionally feature-less in
/// the baseline so the public evidence format never requires redb inspection.
pub mod host_snapshot_store_redb {
    pub const STORE_NAME: &str = "host-snapshot-store-redb";
    pub const ROLE: &str = "optional host-side index/store; not a public evidence boundary and unrelated to chaoscontrol-redb-guest";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn gc_rejects_non_files_before_deleting() {
        let root = tempfile::tempdir().unwrap();
        let snapshots = root.path().join(SNAPSHOT_DIR);
        std::fs::create_dir(&snapshots).unwrap();
        let orphan = snapshots.join("orphan.snapshot.bin");
        std::fs::write(&orphan, b"orphan").unwrap();
        std::fs::create_dir(snapshots.join("stray")).unwrap();
        let store = FileSnapshotStore::new(root.path());
        assert!(matches!(
            store.gc_unreferenced(&[]),
            Err(SnapshotStoreError::NotRegular { .. })
        ));
        assert_eq!(std::fs::read(&orphan).unwrap(), b"orphan");
    }

    #[test]
    fn snapshot_directory_scan_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..MAX_SNAPSHOT_DIRECTORY_ENTRIES {
            std::fs::write(root.path().join(format!("entry-{index}")), b"").unwrap();
        }
        let directory = open_directory(root.path(), false).unwrap();
        assert_eq!(
            list_snapshot_entries(&directory).unwrap().len(),
            MAX_SNAPSHOT_DIRECTORY_ENTRIES
        );
        std::fs::write(root.path().join("one-too-many"), b"").unwrap();
        assert!(list_snapshot_entries(&directory).is_err());
    }

    fn dummy_snapshot(tick: u64) -> SimulationSnapshot {
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());

        SimulationSnapshot {
            tick,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        }
    }

    fn write_artifact(dir: &Path, artifact: &SnapshotArtifactEnvelope) -> ReplayParentSnapshotRef {
        let bytes = zstd::stream::encode_all(
            std::io::Cursor::new(encode_cbor(artifact).unwrap()),
            SNAPSHOT_COMPRESSION_LEVEL,
        )
        .unwrap();
        let digest = digest_bytes(&bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.bin");
        fs::create_dir_all(dir.join("snapshots")).unwrap();
        fs::write(dir.join(&rel), bytes).unwrap();
        ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest,
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: rel,
        }
    }

    #[test]
    fn file_store_validates_metadata_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 2,
                tick: 17,
                vm_count: 0,
                snapshot: dummy_snapshot(17),
            },
        );
        let artifact = store.get_snapshot_artifact(&reference).unwrap();
        assert_eq!(artifact.tick, 17);
        assert_eq!(store.get_snapshot(&reference).unwrap().tick, 17);
        assert_eq!(artifact.replay_parent_depth, 2);
        assert!(store.has_snapshot(&reference));
    }

    #[test]
    fn digest_mismatch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let mut reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 2,
                tick: 17,
                vm_count: 3,
                snapshot: dummy_snapshot(17),
            },
        );
        reference.digest =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn ref_shape_rejects_bad_digest_and_path() {
        let valid = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: format!("sha256:{}", "0".repeat(64)),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/x.snapshot.bin".to_string(),
        };
        validate_ref_shape(&valid).unwrap();

        let mut bad_digest = valid.clone();
        bad_digest.digest = "sha256:not-hex".to_string();
        assert!(matches!(
            validate_ref_shape(&bad_digest),
            Err(SnapshotStoreError::InvalidDigest { .. })
        ));

        let mut bad_path = valid;
        bad_path.path = "../escape.snapshot.bin".to_string();
        assert!(matches!(
            validate_ref_shape(&bad_path),
            Err(SnapshotStoreError::PathEscape { .. })
        ));
    }

    #[test]
    fn path_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:00".to_string(),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "../escape.snapshot".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::PathEscape { .. })
        ));
    }

    #[test]
    fn missing_artifact_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/0000000000000000000000000000000000000000000000000000000000000000.snapshot.bin".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::Missing { .. })
        ));
    }

    #[test]
    fn symlink_snapshot_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let snapshots = dir.path().join(SNAPSHOT_DIR);
        fs::create_dir_all(&snapshots).unwrap();
        let target = snapshots.join("target.snapshot.bin");
        fs::write(&target, b"target").unwrap();
        let link = snapshots.join("link.snapshot.bin");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: digest_bytes(b"target"),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/link.snapshot.bin"),
        };

        assert!(matches!(
            FileSnapshotStore::new(dir.path()).get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::NotRegular { .. })
        ));
    }

    #[test]
    fn oversized_snapshot_is_rejected_before_read() {
        let dir = tempfile::tempdir().unwrap();
        let snapshots = dir.path().join(SNAPSHOT_DIR);
        fs::create_dir_all(&snapshots).unwrap();
        let path = snapshots.join("oversized.snapshot.bin");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_COMPRESSED_SNAPSHOT_BYTES + READ_LIMIT_SENTINEL_BYTES)
            .unwrap();
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: digest_bytes(&[]),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/oversized.snapshot.bin"),
        };

        assert!(matches!(
            FileSnapshotStore::new(dir.path()).get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::TooLarge { .. })
        ));
    }

    #[test]
    fn decompression_limit_is_enforced() {
        const TEST_DECOMPRESSED_LIMIT: u64 = 16;
        let expanded = vec![0_u8; (TEST_DECOMPRESSED_LIMIT + READ_LIMIT_SENTINEL_BYTES) as usize];
        let compressed =
            zstd::stream::encode_all(std::io::Cursor::new(expanded), SNAPSHOT_COMPRESSION_LEVEL)
                .unwrap();

        assert!(matches!(
            decompress_snapshot(&compressed, TEST_DECOMPRESSED_LIMIT),
            Err(SnapshotStoreError::DecompressedTooLarge { .. })
        ));
    }

    #[test]
    fn snapshot_envelope_metadata_mismatch_is_rejected() {
        const FORGED_VM_COUNT: usize = 3;
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 1,
                tick: 17,
                vm_count: FORGED_VM_COUNT,
                snapshot: dummy_snapshot(17),
            },
        );

        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::MetadataMismatch { field: "vm_count" })
        ));
    }

    #[test]
    fn unsupported_codec_and_schema_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:00".to_string(),
            codec: "unsupported".to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/00.snapshot.json".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::UnsupportedCodec { .. })
        ));
        let reference = ReplayParentSnapshotRef {
            schema_version: SNAPSHOT_SCHEMA_VERSION + 1,
            codec: SNAPSHOT_CODEC.to_string(),
            ..reference
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn corrupt_binary_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        let bytes = b"not zstd-compressed bincode";
        let digest = digest_bytes(bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.bin");
        fs::write(dir.path().join(&rel), bytes).unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest,
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: rel,
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::Io { .. })
        ));
    }

    #[test]
    fn reads_reject_length_changes() {
        let file = File::open("/proc/self/cmdline").unwrap();
        assert!(read_snapshot_bytes(file, "proc").is_err());
    }

    #[test]
    fn reads_reject_symlinked_snapshot_directories() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(SNAPSHOT_DIR)).unwrap();
        let store = FileSnapshotStore::new(root.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: format!("sha256:{}", "0".repeat(64)),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/00.snapshot.bin"),
        };

        assert!(matches!(
            store.resolve_ref(&reference),
            Err(SnapshotStoreError::NotDirectory { .. })
        ));
        assert!(!store.has_snapshot(&reference));
    }

    #[test]
    fn nested_snapshot_references_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(SNAPSHOT_DIR)).unwrap();
        std::os::unix::fs::symlink(
            outside.path(),
            root.path().join(SNAPSHOT_DIR).join("nested"),
        )
        .unwrap();
        let store = FileSnapshotStore::new(root.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:00".to_string(),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/nested/00.snapshot.bin"),
        };
        assert!(matches!(
            store.resolve_ref(&reference),
            Err(SnapshotStoreError::PathEscape { .. })
        ));
    }

    #[test]
    fn publication_rejects_symlinked_snapshot_directories() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(SNAPSHOT_DIR)).unwrap();
        let store = FileSnapshotStore::new(root.path());

        assert!(matches!(
            store.put_snapshot(&dummy_snapshot(1), 0),
            Err(SnapshotStoreError::NotDirectory { .. })
        ));
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn gc_rejects_symlinked_snapshot_directories() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("keep.bin");
        fs::write(&outside_file, b"keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(SNAPSHOT_DIR)).unwrap();
        let store = FileSnapshotStore::new(root.path());

        assert!(matches!(
            store.gc_unreferenced(&[]),
            Err(SnapshotStoreError::NotDirectory { .. })
        ));
        assert_eq!(fs::read(&outside_file).unwrap(), b"keep");
    }

    #[test]
    fn gc_rejects_invalid_keep_refs_without_deleting() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let valid = store.put_snapshot(&dummy_snapshot(1), 0).unwrap();
        let mut invalid = valid.clone();
        invalid.digest = "sha256:00".to_string();

        assert!(matches!(
            store.gc_unreferenced(std::slice::from_ref(&invalid)),
            Err(SnapshotStoreError::InvalidDigest { .. })
        ));
        assert!(store.has_snapshot(&valid));
    }

    #[test]
    fn gc_removes_only_unreferenced_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let first = store.put_snapshot(&dummy_snapshot(1), 0).unwrap();
        let second = store.put_snapshot(&dummy_snapshot(2), 0).unwrap();

        assert_eq!(
            store.gc_unreferenced(std::slice::from_ref(&first)).unwrap(),
            1
        );
        assert!(store.has_snapshot(&first));
        assert!(!store.has_snapshot(&second));
    }

    #[test]
    fn all_operations_reject_symlinked_roots_and_ancestors() {
        for ancestor in [false, true] {
            let sandbox = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            let outside_root = if ancestor {
                outside.path().join("run")
            } else {
                outside.path().to_path_buf()
            };
            let outside_store = FileSnapshotStore::new(&outside_root);
            let reference = outside_store.put_snapshot(&dummy_snapshot(17), 0).unwrap();
            let link = sandbox.path().join("link");
            std::os::unix::fs::symlink(outside.path(), &link).unwrap();
            let selected = if ancestor { link.join("run") } else { link };
            let store = FileSnapshotStore::new(selected);

            assert!(matches!(
                store.get_snapshot_artifact(&reference),
                Err(SnapshotStoreError::NotDirectory { .. })
            ));
            assert!(!store.has_snapshot(&reference));
            assert!(matches!(
                store.put_snapshot(&dummy_snapshot(18), 0),
                Err(SnapshotStoreError::NotDirectory { .. })
            ));
            assert!(matches!(
                store.gc_unreferenced(&[]),
                Err(SnapshotStoreError::NotDirectory { .. })
            ));
            assert_eq!(outside_store.get_snapshot(&reference).unwrap().tick, 17);
            assert_eq!(
                fs::read_dir(outside_root.join(SNAPSHOT_DIR))
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    // Deterministically substitute a selected directory after the production
    // shell has pinned it. Both real-directory replacement and symlink
    // redirection must leave the in-flight operation on its original inode.
    fn substitute_directory(sandbox: &Path, ancestor: bool, symlink: bool) -> (PathBuf, PathBuf) {
        let run = sandbox.join("run");
        let selected = if ancestor {
            run
        } else {
            run.join(SNAPSHOT_DIR)
        };
        let detached = sandbox.join("detached");
        fs::rename(&selected, &detached).unwrap();
        let replacement = if symlink {
            let outside = sandbox.join("outside");
            fs::create_dir(&outside).unwrap();
            std::os::unix::fs::symlink(&outside, &selected).unwrap();
            outside
        } else {
            fs::create_dir(&selected).unwrap();
            selected
        };
        if ancestor {
            let replacement_snapshots = replacement.join(SNAPSHOT_DIR);
            fs::create_dir(&replacement_snapshots).unwrap();
            (detached.join(SNAPSHOT_DIR), replacement_snapshots)
        } else {
            (detached, replacement)
        }
    }

    #[test]
    fn publication_stays_in_pinned_directory_after_substitution() {
        for ancestor in [false, true] {
            for symlink in [false, true] {
                let sandbox = tempfile::tempdir().unwrap();
                let store = FileSnapshotStore::new(sandbox.path().join("run"));
                let directory = store.snapshots_directory(true).unwrap();
                let (detached, replacement) =
                    substitute_directory(sandbox.path(), ancestor, symlink);

                let reference =
                    put_snapshot_in_directory(&directory, &dummy_snapshot(29), 2).unwrap();
                let name = Path::new(&reference.path).file_name().unwrap();
                let bytes = fs::read(detached.join(name)).unwrap();
                assert_eq!(
                    decode_snapshot_artifact(&reference, &bytes).unwrap().tick,
                    29
                );
                assert_eq!(fs::read_dir(&replacement).unwrap().count(), 0);
                // Staging and publication both used the retained capability.
                assert_eq!(fs::read_dir(&detached).unwrap().count(), 1);
            }
        }
    }

    #[test]
    fn reads_stay_in_pinned_directory_after_substitution() {
        for ancestor in [false, true] {
            for symlink in [false, true] {
                let sandbox = tempfile::tempdir().unwrap();
                let store = FileSnapshotStore::new(sandbox.path().join("run"));
                let reference = store.put_snapshot(&dummy_snapshot(31), 1).unwrap();
                let (directory, name) = store.resolve_ref(&reference).unwrap();
                let (_, replacement) = substitute_directory(sandbox.path(), ancestor, symlink);
                fs::write(replacement.join(&name), b"substitute").unwrap();

                let file = open_snapshot_file(&directory, &name, &reference.path).unwrap();
                let bytes = read_snapshot_bytes(file, &reference.path).unwrap();
                assert_eq!(
                    decode_snapshot_artifact(&reference, &bytes).unwrap().tick,
                    31
                );
                assert_eq!(fs::read(replacement.join(&name)).unwrap(), b"substitute");
                assert!(store.get_snapshot_artifact(&reference).is_err());
            }
        }
    }

    #[test]
    fn gc_stays_in_pinned_directory_after_substitution() {
        for ancestor in [false, true] {
            for symlink in [false, true] {
                let sandbox = tempfile::tempdir().unwrap();
                let store = FileSnapshotStore::new(sandbox.path().join("run"));
                let kept = store.put_snapshot(&dummy_snapshot(37), 0).unwrap();
                let orphan = store.put_snapshot(&dummy_snapshot(41), 0).unwrap();
                let directory = store.snapshots_directory(false).unwrap();
                let (detached, replacement) =
                    substitute_directory(sandbox.path(), ancestor, symlink);
                let orphan_name = Path::new(&orphan.path).file_name().unwrap();
                let kept_name = Path::new(&kept.path).file_name().unwrap();
                fs::write(replacement.join(orphan_name), b"outside orphan").unwrap();
                fs::write(replacement.join(kept_name), b"outside kept").unwrap();

                assert_eq!(
                    gc_in_directory(&directory, std::slice::from_ref(&kept)).unwrap(),
                    1
                );
                assert!(!detached.join(orphan_name).exists());
                assert!(detached.join(kept_name).is_file());
                assert_eq!(
                    fs::read(replacement.join(orphan_name)).unwrap(),
                    b"outside orphan"
                );
                assert_eq!(
                    fs::read(replacement.join(kept_name)).unwrap(),
                    b"outside kept"
                );
                // Repeated enumeration must start at zero on the pinned inode.
                assert_eq!(
                    gc_in_directory(&directory, std::slice::from_ref(&kept)).unwrap(),
                    0
                );
            }
        }
    }

    #[test]
    fn repeated_snapshot_publication_leaves_no_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let snapshot = dummy_snapshot(99);

        let first = store.put_snapshot(&snapshot, 1).unwrap();
        let second = store.put_snapshot(&snapshot, 1).unwrap();
        assert_eq!(first, second);

        let names = fs::read_dir(dir.path().join(SNAPSHOT_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 1);
        assert!(names[0].ends_with(".snapshot.bin"));
    }
}
