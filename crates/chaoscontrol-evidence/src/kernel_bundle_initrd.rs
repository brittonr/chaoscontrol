use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use bounded_tree_cap::{execute as execute_tree_copy, prepare as prepare_tree};
use bounded_tree_core::{LimitValues, RelativePath, SymlinkPolicy, TreeLimits, TreePlan};
use cap_std::{ambient_authority, fs::Dir};
use serde::Serialize;

use crate::{EvidenceError, EvidenceResult};

pub const PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE: &str = "6.18.20";
pub const PRIVATE_KFUNC_MODULE_FILE: &str = "private_kfunc.mod.ko";
pub const PRIVATE_KFUNC_BPF_FILE: &str = "private_kfunc.ebpf.o";
pub const PRIVATE_KFUNC_LOADER_FILE: &str = "private_kfunc";
pub const PRIVATE_KFUNC_BPFFS_PIN: &str = "/sys/fs/bpf/mantle-kfunc";
pub const PRIVATE_KFUNC_INITRD_SCHEMA_VERSION: u64 = 2;

const NEWC_MAGIC: &str = "070701";
const NEWC_HEADER_LEN: u64 = 110;
const NEWC_TRAILER: &str = "TRAILER!!!";
const NEWC_ALIGNMENT: u64 = 4;
const INIT_SCRIPT_PATH: &str = "init";
const BUSYBOX_GUEST_PATH: &str = "bin/busybox";
const ARTIFACT_GUEST_DIR: &str = "artifacts";
const VERIFIER_REJECT_ARTIFACT_PATH: &str = "artifacts/verifier-reject.ebpf.o";
const VERIFIER_REJECT_ARTIFACT_BYTES: &[u8] = b"not-an-elf-bpf-object\n";
const SUPPORTED_SCENARIOS: &[&str] = &[
    "positive",
    "missing-kfunc",
    "verifier-rejection",
    "wrong-attach-target",
    "cleanup-failure",
];
const ROOT_UID: u32 = 0;
const ROOT_GID: u32 = 0;
const DEFAULT_INODE_START: u32 = 1;
const MODE_REGULAR_FILE: u32 = 0o100000;
const MODE_DIRECTORY: u32 = 0o040000;
const MODE_SYMLINK: u32 = 0o120000;
const MODE_UNSUPPORTED_MASK: u32 = 0o170000;
const DEFAULT_FILE_MODE: u32 = MODE_REGULAR_FILE | 0o644;
const EXECUTABLE_FILE_MODE: u32 = MODE_REGULAR_FILE | 0o755;
const DEFAULT_DIR_MODE: u32 = MODE_DIRECTORY | 0o755;
const DEFAULT_SYMLINK_MODE: u32 = MODE_SYMLINK | 0o777;
const DEFAULT_MTIME_SECS: u32 = 1;
const FILE_COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_INITRD_ENTRIES: usize = 100_000;
const MAX_INITRD_TREE_DEPTH: u64 = 128;
const MAX_INITRD_MEMBER_BYTES: u64 = u32::MAX as u64;
const MAX_INITRD_TREE_BYTES: u64 = u32::MAX as u64;
const MAX_INITRD_ARCHIVE_BYTES: u64 = u32::MAX as u64;
const MAX_CLOSURE_LIST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SCRIPT_BYTES: usize = 16 * 1024;
const MAX_ARCHIVE_PATH_BYTES: usize = 512;
const MAX_SYMLINK_TARGET_BYTES: usize = 4096;
const MAX_CLOSURE_ROOTS: usize = 4096;

const ROOT_DIRS: &[&str] = &[
    "bin",
    "dev",
    "proc",
    "run",
    "sys",
    "sys/fs",
    "sys/fs/bpf",
    "tmp",
    ARTIFACT_GUEST_DIR,
    "nix",
    "nix/store",
];

const BUSYBOX_APPLETS: &[&str] = &[
    "cat",
    "dmesg",
    "grep",
    "halt",
    "insmod",
    "ip",
    "mkdir",
    "mount",
    "mountpoint",
    "poweroff",
    "reboot",
    "rm",
    "sh",
    "sleep",
    "sync",
    "uname",
];

macro_rules! ensure {
    ($condition:expr, $message:expr) => {
        if $condition {
            Ok(())
        } else {
            Err(EvidenceError::new($message))
        }
    };
}

#[derive(Debug, Clone)]
pub struct PrivateKfuncInitrdRequest<'a> {
    pub output_path: &'a Path,
    pub artifacts_dir: &'a Path,
    pub busybox_path: &'a Path,
    pub bpftool_path: &'a Path,
    pub delete_module_helper_path: &'a Path,
    pub closure_list_path: &'a Path,
    pub expected_kernel_release: &'a str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrivateKfuncInitrdSummary {
    pub schema_version: u64,
    pub output_path: String,
    pub expected_kernel_release: String,
    pub bpftool_guest_path: String,
    pub delete_module_helper_guest_path: String,
    pub entries_written: usize,
    pub archive_bytes: u64,
    pub closure_roots: usize,
    pub artifact_files: Vec<String>,
    pub supported_scenarios: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Directory,
    Regular,
    Symlink,
}

pub fn private_kfunc_init_script(
    bpftool_guest_path: &str,
    delete_module_helper_guest_path: &str,
    expected_kernel_release: &str,
) -> EvidenceResult<String> {
    ensure_non_empty(bpftool_guest_path, "bpftool guest path")?;
    ensure_non_empty(
        delete_module_helper_guest_path,
        "delete-module helper guest path",
    )?;
    ensure_non_empty(expected_kernel_release, "expected kernel release")?;
    let mut script = String::new();
    script.push_str("#!/bin/sh\n");
    script.push_str("set +e\n");
    script.push_str("export PATH=/bin:/sbin\n");
    script.push_str(&format!(
        "EXPECTED_KERNEL_RELEASE='{}'\n",
        shell_single_quote(expected_kernel_release)
    ));
    script.push_str(&format!(
        "BPFTOOL='{}'\n",
        shell_single_quote(bpftool_guest_path)
    ));
    script.push_str(&format!(
        "DELETE_MODULE='{}'\n",
        shell_single_quote(delete_module_helper_guest_path)
    ));
    script.push_str("MARKER_PREFIX='chaoscontrol-kernel-bundle:v1:'\n");
    script.push_str("CONSOLE=/dev/ttyS0\n");
    script.push_str("PIN='");
    script.push_str(PRIVATE_KFUNC_BPFFS_PIN);
    script.push_str("'\n");
    script.push_str("CASE=positive\n");
    script.push_str("marker() { printf '%scase=%s;class=%s;detail=%s\\n' \"$MARKER_PREFIX\" \"$1\" \"$2\" \"$3\" >\"$CONSOLE\"; sleep 1; }\n");
    script.push_str("finish() { sleep 1; sync; poweroff -f 2>/dev/null || reboot -f 2>/dev/null || halt -f 2>/dev/null; while true; do sleep 1; done; }\n");
    script.push_str("fail() { marker \"$1\" error \"$2\"; finish; }\n");
    script.push_str("find_module_name() { MODULE_NAME=''; while read -r module_name _rest; do case \"$module_name\" in private_kfunc*) MODULE_NAME=\"$module_name\"; break;; esac; done </proc/modules; }\n");
    script.push_str("unload_module() { find_module_name; if [ -n \"$MODULE_NAME\" ]; then \"$DELETE_MODULE\" \"$MODULE_NAME\"; else return 1; fi; }\n");
    script.push_str("mount -t proc proc /proc || fail boot mount-proc-failed\n");
    script.push_str("for arg in $(cat /proc/cmdline); do case \"$arg\" in chaos_kernel_bundle_case=*) CASE=${arg#*=};; esac; done\n");
    script.push_str("case \"$CASE\" in positive|missing-kfunc|verifier-rejection|wrong-attach-target|cleanup-failure) ;; *) fail boot unsupported-scenario;; esac\n");
    script.push_str("dmesg -n 1 2>/dev/null || true\n");
    script.push_str("mount -t sysfs sysfs /sys || fail boot mount-sysfs-failed\n");
    script.push_str("mount -t devtmpfs devtmpfs /dev 2>/dev/null || true\n");
    script.push_str("mkdir -p /sys/fs/bpf /tmp || fail boot mkdir-runtime-failed\n");
    script.push_str("mountpoint -q /sys/fs/bpf || mount -t bpf bpf /sys/fs/bpf || fail bpf mount-bpffs-failed\n");
    script.push_str("ip link set lo up 2>/tmp/lo-up.err || true\n");
    script.push_str("if [ \"$(uname -r)\" = \"$EXPECTED_KERNEL_RELEASE\" ]; then marker boot ready uname-r-matched; else fail boot uname-r-mismatch; fi\n");
    script.push_str("if [ \"$CASE\" = missing-kfunc ]; then if \"$BPFTOOL\" prog load /artifacts/private_kfunc.ebpf.o \"$PIN\" type xdp >/tmp/missing-kfunc.out 2>&1; then rm -f \"$PIN\"; fail bpf missing-kfunc-unexpectedly-admitted; else cat /tmp/missing-kfunc.out; fail bpf missing-kfunc-rejected; fi; fi\n");
    script.push_str("if [ \"$CASE\" = verifier-rejection ]; then if \"$BPFTOOL\" prog load /artifacts/verifier-reject.ebpf.o \"$PIN\" type xdp >/tmp/verifier-reject.out 2>&1; then rm -f \"$PIN\"; fail bpf malformed-object-unexpectedly-admitted; else cat /tmp/verifier-reject.out; fail bpf verifier-rejected; fi; fi\n");
    script.push_str(
        "insmod /artifacts/private_kfunc.mod.ko || fail module insmod-private-kfunc-failed\n",
    );
    script.push_str("marker module load insmod-private-kfunc-succeeded\n");
    script.push_str("\"$BPFTOOL\" prog load /artifacts/private_kfunc.ebpf.o \"$PIN\" type xdp || fail bpf bpftool-prog-load-xdp-failed\n");
    script.push_str("marker bpf verify bpftool-prog-load-xdp-succeeded\n");
    script.push_str("if [ \"$CASE\" = wrong-attach-target ]; then if \"$BPFTOOL\" net attach xdp pinned \"$PIN\" dev chaos-missing0 >/tmp/wrong-target.out 2>&1; then fail bpf wrong-attach-target-unexpectedly-admitted; else cat /tmp/wrong-target.out; rm -f \"$PIN\" || true; unload_module || true; fail bpf wrong-attach-target-rejected; fi; fi\n");
    script.push_str("cd /artifacts || fail bpf artifacts-chdir-failed\n");
    script.push_str("./private_kfunc > /tmp/private_kfunc.out 2>&1\n");
    script.push_str("loader_status=$?\n");
    script.push_str("cat /tmp/private_kfunc.out\n");
    script.push_str(
        "if [ \"$loader_status\" -ne 0 ]; then fail bpf private-kfunc-loader-failed; fi\n",
    );
    script.push_str("grep -q 'TCP monitor attached with private kfunc capabilities' /tmp/private_kfunc.out || fail bpf private-kfunc-loader-attach-marker-missing\n");
    script.push_str("marker bpf attach private-kfunc-loader-attached\n");
    script.push_str("grep -q 'TCP monitor detached' /tmp/private_kfunc.out || fail bpf private-kfunc-loader-detach-marker-missing\n");
    script.push_str("marker bpf detach private-kfunc-loader-detached\n");
    script.push_str("if [ \"$CASE\" = cleanup-failure ]; then if rm -f /sys/fs/bpf >/tmp/cleanup-failure.out 2>&1; then fail bpf cleanup-unexpectedly-succeeded; else cat /tmp/cleanup-failure.out; fail bpf cleanup-failed; fi; fi\n");
    script.push_str("rm -f \"$PIN\" || fail bpf pinned-prog-cleanup-failed\n");
    script.push_str("marker bpf cleanup succeeded\n");
    script.push_str("unload_module || fail module rmmod-private-kfunc-failed\n");
    script.push_str("marker module unload rmmod-private-kfunc-attempted\n");
    script.push_str("marker module cleanup succeeded\n");
    script.push_str("finish\n");
    ensure!(
        script.len() <= MAX_SCRIPT_BYTES,
        "private-kfunc init script exceeded size bound"
    )?;
    Ok(script)
}

pub fn write_private_kfunc_initrd(
    request: &PrivateKfuncInitrdRequest<'_>,
) -> EvidenceResult<PrivateKfuncInitrdSummary> {
    validate_request(request)?;
    if let Some(parent) = request.output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let closure_roots = read_closure_roots(request.closure_list_path)?;
    let artifact_files = private_kfunc_artifact_paths(request.artifacts_dir)?;
    let output_file = File::create(request.output_path)?;
    let mut writer = NewcWriter::new(output_file);
    let mut seen = BTreeSet::new();

    for dir in ROOT_DIRS {
        writer.add_static_entry(&mut seen, dir, EntryKind::Directory, DEFAULT_DIR_MODE, &[])?;
    }
    let delete_module_helper_guest_path = "/bin/kernel-bundle-delete-module";
    let script = private_kfunc_init_script(
        &absolute_guest_path(request.bpftool_path)?,
        delete_module_helper_guest_path,
        request.expected_kernel_release,
    )?;
    writer.add_static_entry(
        &mut seen,
        INIT_SCRIPT_PATH,
        EntryKind::Regular,
        EXECUTABLE_FILE_MODE,
        script.as_bytes(),
    )?;
    writer.add_file_at(&mut seen, request.busybox_path, BUSYBOX_GUEST_PATH, true)?;
    let bpftool_archive_path = archive_path_for_absolute(request.bpftool_path)?;
    writer.add_path(&mut seen, request.bpftool_path, &bpftool_archive_path, true)?;
    writer.add_file_at(
        &mut seen,
        request.delete_module_helper_path,
        "bin/kernel-bundle-delete-module",
        true,
    )?;
    for applet in BUSYBOX_APPLETS {
        let link_path = format!("bin/{applet}");
        if link_path != BUSYBOX_GUEST_PATH {
            writer.add_static_entry(
                &mut seen,
                &link_path,
                EntryKind::Symlink,
                DEFAULT_SYMLINK_MODE,
                b"busybox",
            )?;
        }
    }
    for artifact in &artifact_files {
        let name = file_name_string(artifact)?;
        let guest_path = format!("{ARTIFACT_GUEST_DIR}/{name}");
        let executable = name == PRIVATE_KFUNC_LOADER_FILE;
        writer.add_file_at(&mut seen, artifact, &guest_path, executable)?;
    }
    writer.add_static_entry(
        &mut seen,
        VERIFIER_REJECT_ARTIFACT_PATH,
        EntryKind::Regular,
        DEFAULT_FILE_MODE,
        VERIFIER_REJECT_ARTIFACT_BYTES,
    )?;
    for root in &closure_roots {
        writer.add_absolute_tree(&mut seen, root)?;
    }
    writer.finish(&mut seen)?;

    Ok(PrivateKfuncInitrdSummary {
        schema_version: PRIVATE_KFUNC_INITRD_SCHEMA_VERSION,
        output_path: request.output_path.display().to_string(),
        expected_kernel_release: request.expected_kernel_release.to_string(),
        bpftool_guest_path: absolute_guest_path(request.bpftool_path)?,
        delete_module_helper_guest_path: delete_module_helper_guest_path.to_string(),
        entries_written: writer.entries_written,
        archive_bytes: writer.bytes_written,
        closure_roots: closure_roots.len(),
        artifact_files: artifact_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        supported_scenarios: SUPPORTED_SCENARIOS
            .iter()
            .map(|scenario| (*scenario).to_string())
            .collect(),
    })
}

fn validate_request(request: &PrivateKfuncInitrdRequest<'_>) -> EvidenceResult<()> {
    ensure_non_empty(request.expected_kernel_release, "expected kernel release")?;
    ensure_file(request.busybox_path, "busybox")?;
    ensure_file(request.bpftool_path, "bpftool")?;
    ensure_file(request.delete_module_helper_path, "delete-module helper")?;
    ensure_dir(request.artifacts_dir, "artifacts dir")?;
    ensure_file(request.closure_list_path, "closure list")?;
    Ok(())
}

fn private_kfunc_artifact_paths(artifacts_dir: &Path) -> EvidenceResult<Vec<PathBuf>> {
    let names = [
        PRIVATE_KFUNC_MODULE_FILE,
        PRIVATE_KFUNC_BPF_FILE,
        PRIVATE_KFUNC_LOADER_FILE,
    ];
    let mut paths = Vec::new();
    for name in names {
        let path = artifacts_dir.join(name);
        ensure_file(&path, name)?;
        paths.push(path);
    }
    Ok(paths)
}

fn read_closure_roots(path: &Path) -> EvidenceResult<Vec<PathBuf>> {
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.len() <= MAX_CLOSURE_LIST_BYTES,
        "closure list exceeded size bound"
    )?;
    let content = fs::read_to_string(path)?;
    let mut roots = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let root = PathBuf::from(trimmed);
        ensure!(
            root.is_absolute(),
            format!("closure root is not absolute: {trimmed}")
        )?;
        ensure!(
            root.exists(),
            format!("closure root does not exist: {trimmed}")
        )?;
        roots.push(root);
    }
    roots.sort();
    roots.dedup();
    ensure!(roots.len() <= MAX_CLOSURE_ROOTS, "too many closure roots")?;
    Ok(roots)
}

fn absolute_guest_path(path: &Path) -> EvidenceResult<String> {
    ensure!(
        path.is_absolute(),
        format!("guest path is not absolute: {}", path.display())
    )?;
    Ok(path.display().to_string())
}

fn ensure_file(path: &Path, label: &str) -> EvidenceResult<()> {
    ensure!(
        path.is_file(),
        format!("{label} is not a regular file: {}", path.display())
    )
}

fn ensure_dir(path: &Path, label: &str) -> EvidenceResult<()> {
    ensure!(
        path.is_dir(),
        format!("{label} is not a directory: {}", path.display())
    )
}

fn ensure_non_empty(value: &str, label: &str) -> EvidenceResult<()> {
    ensure!(!value.is_empty(), format!("{label} must not be empty"))
}

fn file_name_string(path: &Path) -> EvidenceResult<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            EvidenceError::new(format!("path has no UTF-8 filename: {}", path.display()))
        })
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn bounded_tree_limits() -> EvidenceResult<TreeLimits> {
    TreeLimits::new(LimitValues {
        entries: u64::try_from(MAX_INITRD_ENTRIES)
            .map_err(|_| EvidenceError::new("initrd entry bound does not fit u64"))?,
        depth: MAX_INITRD_TREE_DEPTH,
        path_bytes: u64::try_from(MAX_ARCHIVE_PATH_BYTES)
            .map_err(|_| EvidenceError::new("archive path bound does not fit u64"))?,
        file_bytes: MAX_INITRD_MEMBER_BYTES,
        total_bytes: MAX_INITRD_TREE_BYTES,
        symlink_target_bytes: u64::try_from(MAX_SYMLINK_TARGET_BYTES)
            .map_err(|_| EvidenceError::new("symlink target bound does not fit u64"))?,
    })
    .map_err(|error| EvidenceError::new(format!("invalid bounded-tree limits: {error:?}")))
}

fn prepare_bounded_tree(root: &Path) -> EvidenceResult<(tempfile::TempDir, TreePlan)> {
    let source = Dir::open_ambient_dir(root, ambient_authority())?;
    let prepared = prepare_tree(
        &source,
        bounded_tree_limits()?,
        SymlinkPolicy::PreserveInternal,
    )
    .map_err(|error| EvidenceError::new(format!("bounded-tree observation failed: {error:?}")))?;
    let staging = tempfile::tempdir()?;
    let destination = Dir::open_ambient_dir(staging.path(), ambient_authority())?;
    execute_tree_copy(&prepared, &destination).map_err(|error| {
        EvidenceError::new(format!("bounded-tree revalidation failed: {error:?}"))
    })?;
    let plan = prepared.plan().clone();
    Ok((staging, plan))
}

fn relative_path_buf(path: &RelativePath) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        result.push(std::ffi::OsString::from_vec(component.clone()));
    }
    result
}

struct NewcWriter<W: Write> {
    output: W,
    next_inode: u32,
    entries_written: usize,
    bytes_written: u64,
}

impl<W: Write> NewcWriter<W> {
    fn new(output: W) -> Self {
        Self {
            output,
            next_inode: DEFAULT_INODE_START,
            entries_written: 0,
            bytes_written: 0,
        }
    }

    fn add_absolute_tree(
        &mut self,
        seen: &mut BTreeSet<String>,
        root: &Path,
    ) -> EvidenceResult<()> {
        let archive_path = archive_path_for_absolute(root)?;
        if !root.is_dir() {
            return self.add_path(seen, root, &archive_path, false);
        }
        let (staging, plan) = prepare_bounded_tree(root)?;
        self.add_parent_dirs(seen, &archive_path)?;
        self.add_static_entry(
            seen,
            &archive_path,
            EntryKind::Directory,
            DEFAULT_DIR_MODE,
            &[],
        )?;
        for member in plan.member_facts() {
            let relative = relative_path_buf(member.path());
            let child_archive_path = join_archive_path(&archive_path, &relative)?;
            let staged_source = staging.path().join(&relative);
            self.add_path(seen, &staged_source, &child_archive_path, false)?;
        }
        Ok(())
    }

    fn add_file_at(
        &mut self,
        seen: &mut BTreeSet<String>,
        source: &Path,
        archive_path: &str,
        executable: bool,
    ) -> EvidenceResult<()> {
        validate_archive_path(archive_path)?;
        self.add_parent_dirs(seen, archive_path)?;
        let mode = if executable {
            EXECUTABLE_FILE_MODE
        } else {
            DEFAULT_FILE_MODE
        };
        self.add_regular_file(seen, archive_path, source, mode)
    }

    fn add_path(
        &mut self,
        seen: &mut BTreeSet<String>,
        source: &Path,
        archive_path: &str,
        executable_override: bool,
    ) -> EvidenceResult<()> {
        validate_archive_path(archive_path)?;
        self.add_parent_dirs(seen, archive_path)?;
        let metadata = fs::symlink_metadata(source)?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            return self.add_static_entry(
                seen,
                archive_path,
                EntryKind::Directory,
                DEFAULT_DIR_MODE,
                &[],
            );
        }
        if file_type.is_symlink() {
            let target = fs::read_link(source)?;
            let bytes = path_to_bytes(&target)?;
            ensure!(
                bytes.len() <= MAX_SYMLINK_TARGET_BYTES,
                "symlink target exceeded size bound"
            )?;
            return self.add_static_entry(
                seen,
                archive_path,
                EntryKind::Symlink,
                DEFAULT_SYMLINK_MODE,
                &bytes,
            );
        }
        if file_type.is_file() {
            let mode = archive_regular_mode(&metadata, executable_override);
            return self.add_regular_file(seen, archive_path, source, mode);
        }
        unsupported_file_error(source, metadata.mode())
    }

    fn add_parent_dirs(
        &mut self,
        seen: &mut BTreeSet<String>,
        archive_path: &str,
    ) -> EvidenceResult<()> {
        let mut current = String::new();
        for part in archive_path.split('/').take_while(|part| !part.is_empty()) {
            if current.is_empty() {
                current.push_str(part);
            } else {
                current.push('/');
                current.push_str(part);
            }
            if current == archive_path {
                break;
            }
            self.add_static_entry(seen, &current, EntryKind::Directory, DEFAULT_DIR_MODE, &[])?;
        }
        Ok(())
    }

    fn add_regular_file(
        &mut self,
        seen: &mut BTreeSet<String>,
        archive_path: &str,
        source: &Path,
        mode: u32,
    ) -> EvidenceResult<()> {
        if !seen.insert(archive_path.to_string()) {
            return Ok(());
        }
        self.check_entry_limit()?;
        let mut file = File::open(source)?;
        let size = file.metadata()?.len();
        self.write_header(archive_path, mode, size, EntryKind::Regular)?;
        self.copy_file_data(&mut file)?;
        self.write_padding(size)?;
        self.entries_written += 1;
        Ok(())
    }

    fn add_static_entry(
        &mut self,
        seen: &mut BTreeSet<String>,
        archive_path: &str,
        kind: EntryKind,
        mode: u32,
        data: &[u8],
    ) -> EvidenceResult<()> {
        validate_archive_path(archive_path)?;
        if !seen.insert(archive_path.to_string()) {
            return Ok(());
        }
        self.check_entry_limit()?;
        let data_bytes = u64::try_from(data.len())
            .map_err(|_| EvidenceError::new("newc entry length does not fit u64"))?;
        self.write_header(archive_path, mode, data_bytes, kind)?;
        self.write_output(data)?;
        self.write_padding(data_bytes)?;
        self.entries_written += 1;
        Ok(())
    }

    fn finish(&mut self, seen: &mut BTreeSet<String>) -> EvidenceResult<()> {
        self.add_static_entry(
            seen,
            NEWC_TRAILER,
            EntryKind::Regular,
            DEFAULT_FILE_MODE,
            &[],
        )?;
        self.output.flush()?;
        Ok(())
    }

    fn write_header(
        &mut self,
        archive_path: &str,
        mode: u32,
        file_size: u64,
        kind: EntryKind,
    ) -> EvidenceResult<()> {
        ensure!(
            file_size <= MAX_INITRD_MEMBER_BYTES,
            "newc member exceeded size bound"
        )?;
        let inode = self.take_inode()?;
        let namesize = archive_path
            .len()
            .checked_add(1)
            .ok_or_else(|| EvidenceError::new("newc name size overflow"))?;
        ensure!(
            namesize <= u32::MAX as usize,
            "newc name exceeded field bound"
        )?;
        let nlink = if kind == EntryKind::Directory { 2 } else { 1 };
        let header = format!(
            "{NEWC_MAGIC}{inode:08x}{mode:08x}{ROOT_UID:08x}{ROOT_GID:08x}{nlink:08x}{DEFAULT_MTIME_SECS:08x}{file_size:08x}{:08x}{:08x}{:08x}{:08x}{namesize:08x}{:08x}",
            0, 0, 0, 0, 0
        );
        ensure!(
            header.len() as u64 == NEWC_HEADER_LEN,
            "newc header length changed"
        )?;
        self.write_output(header.as_bytes())?;
        self.write_output(archive_path.as_bytes())?;
        self.write_output(&[0])?;
        let namesize = u64::try_from(namesize)
            .map_err(|_| EvidenceError::new("newc name size does not fit u64"))?;
        let header_and_name = NEWC_HEADER_LEN
            .checked_add(namesize)
            .ok_or_else(|| EvidenceError::new("newc header accounting overflow"))?;
        self.write_padding(header_and_name)?;
        Ok(())
    }

    fn copy_file_data(&mut self, input: &mut impl Read) -> EvidenceResult<()> {
        let mut buffer = [0_u8; FILE_COPY_BUFFER_BYTES];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            self.write_output(&buffer[..read])?;
        }
        Ok(())
    }

    fn write_output(&mut self, bytes: &[u8]) -> EvidenceResult<()> {
        let byte_count = u64::try_from(bytes.len())
            .map_err(|_| EvidenceError::new("newc write length does not fit u64"))?;
        let next_total = self
            .bytes_written
            .checked_add(byte_count)
            .ok_or_else(|| EvidenceError::new("newc output byte count overflow"))?;
        ensure!(
            next_total <= MAX_INITRD_ARCHIVE_BYTES,
            "initrd archive exceeded output byte bound"
        )?;
        self.output.write_all(bytes)?;
        self.bytes_written = next_total;
        Ok(())
    }

    fn write_padding(&mut self, size: u64) -> EvidenceResult<()> {
        let padding = padding_len(size);
        if padding > 0 {
            let padding = usize::try_from(padding)
                .map_err(|_| EvidenceError::new("newc padding does not fit usize"))?;
            let bytes = vec![0_u8; padding];
            self.write_output(&bytes)?;
        }
        Ok(())
    }

    fn take_inode(&mut self) -> EvidenceResult<u32> {
        let inode = self.next_inode;
        self.next_inode = self
            .next_inode
            .checked_add(1)
            .ok_or_else(|| EvidenceError::new("newc inode counter overflow"))?;
        Ok(inode)
    }

    fn check_entry_limit(&self) -> EvidenceResult<()> {
        ensure!(
            self.entries_written < MAX_INITRD_ENTRIES,
            "initrd entry count exceeded bound"
        )
    }
}

fn archive_regular_mode(metadata: &fs::Metadata, executable_override: bool) -> u32 {
    if executable_override {
        return EXECUTABLE_FILE_MODE;
    }
    MODE_REGULAR_FILE | (metadata.permissions().mode() & 0o777)
}

fn archive_path_for_absolute(path: &Path) -> EvidenceResult<String> {
    ensure!(
        path.is_absolute(),
        format!("path is not absolute: {}", path.display())
    )?;
    let stripped = path.strip_prefix("/").map_err(|err| {
        EvidenceError::new(format!(
            "failed to strip absolute path {}: {err}",
            path.display()
        ))
    })?;
    join_archive_path("", stripped)
}

fn join_archive_path(prefix: &str, relative: &Path) -> EvidenceResult<String> {
    let mut parts: Vec<String> = if prefix.is_empty() {
        Vec::new()
    } else {
        prefix.split('/').map(ToOwned::to_owned).collect()
    };
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(path_component_to_string(value)?),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(EvidenceError::new(format!(
                    "unsupported archive path component in {}",
                    relative.display()
                )));
            }
        }
    }
    let joined = parts.join("/");
    validate_archive_path(&joined)?;
    Ok(joined)
}

fn validate_archive_path(path: &str) -> EvidenceResult<()> {
    ensure!(!path.is_empty(), "archive path must not be empty")?;
    ensure!(
        !path.starts_with('/'),
        format!("archive path is absolute: {path}")
    )?;
    ensure!(
        path.len() <= MAX_ARCHIVE_PATH_BYTES,
        "archive path exceeded size bound"
    )?;
    ensure!(
        !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".."),
        format!("archive path contains unsafe component: {path}")
    )
}

fn path_component_to_string(value: &std::ffi::OsStr) -> EvidenceResult<String> {
    value
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| EvidenceError::new("archive path component is not UTF-8"))
}

fn path_to_bytes(path: &Path) -> EvidenceResult<Vec<u8>> {
    path.to_str()
        .map(|value| value.as_bytes().to_vec())
        .ok_or_else(|| EvidenceError::new(format!("path is not UTF-8: {}", path.display())))
}

fn padding_len(size: u64) -> u64 {
    let remainder = size % NEWC_ALIGNMENT;
    if remainder == 0 {
        0
    } else {
        NEWC_ALIGNMENT - remainder
    }
}

fn unsupported_file_error<T>(path: &Path, mode: u32) -> EvidenceResult<T> {
    let file_type = mode & MODE_UNSUPPORTED_MASK;
    Err(EvidenceError::new(format!(
        "unsupported initrd input type {file_type:o}: {}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs as unix_fs;

    #[test]
    fn init_script_contains_structured_private_kfunc_markers() {
        let script = private_kfunc_init_script(
            "/nix/store/example-bpftool/bin/bpftool",
            "/bin/kernel-bundle-delete-module",
            PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE,
        )
        .expect("script");

        assert!(script.contains("chaoscontrol-kernel-bundle:v1:"));
        assert!(script.contains("marker boot ready uname-r-matched"));
        assert!(script.contains("marker module load insmod-private-kfunc-succeeded"));
        assert!(script.contains("marker bpf verify bpftool-prog-load-xdp-succeeded"));
        assert!(script.contains("marker bpf attach private-kfunc-loader-attached"));
        assert!(script.contains("chaos_kernel_bundle_case="));
        assert!(script.contains("fail bpf missing-kfunc-rejected"));
        assert!(script.contains("fail bpf verifier-rejected"));
        assert!(script.contains("fail bpf wrong-attach-target-rejected"));
        assert!(script.contains("fail bpf cleanup-failed"));
        assert!(script.contains("marker bpf detach private-kfunc-loader-detached"));
        assert!(script.contains("marker module cleanup succeeded"));
        assert!(script.len() <= MAX_SCRIPT_BYTES);
    }

    #[test]
    fn init_script_rejects_empty_inputs() {
        let missing_tool = private_kfunc_init_script(
            "",
            "/bin/kernel-bundle-delete-module",
            PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE,
        )
        .expect_err("missing tool path rejected");
        let missing_rmmod =
            private_kfunc_init_script("/bin/bpftool", "", PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE)
                .expect_err("missing rmmod path rejected");
        let missing_release =
            private_kfunc_init_script("/bin/bpftool", "/bin/kernel-bundle-delete-module", "")
                .expect_err("missing release rejected");

        assert!(missing_tool.message().contains("bpftool guest path"));
        assert!(missing_rmmod
            .message()
            .contains("delete-module helper guest path"));
        assert!(missing_release
            .message()
            .contains("expected kernel release"));
    }

    fn legacy_collect_tree(root: &Path) -> EvidenceResult<Vec<PathBuf>> {
        let mut pending = vec![root.to_path_buf()];
        let mut entries = Vec::new();
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                let metadata = fs::symlink_metadata(&path)?;
                entries.push(path.clone());
                if metadata.file_type().is_dir() {
                    pending.push(path);
                }
            }
        }
        entries.sort();
        Ok(entries)
    }

    fn legacy_archive_tree(root: &Path) -> EvidenceResult<Vec<u8>> {
        let mut output = Vec::new();
        {
            let mut writer = NewcWriter::new(&mut output);
            let mut seen = BTreeSet::new();
            let archive_path = archive_path_for_absolute(root)?;
            writer.add_path(&mut seen, root, &archive_path, false)?;
            for entry in legacy_collect_tree(root)? {
                let relative = entry.strip_prefix(root).map_err(|error| {
                    EvidenceError::new(format!("legacy fixture strip failed: {error}"))
                })?;
                let child_archive_path = join_archive_path(&archive_path, relative)?;
                writer.add_path(&mut seen, &entry, &child_archive_path, false)?;
            }
            writer.finish(&mut seen)?;
        }
        Ok(output)
    }

    fn archive_tree(root: &Path) -> EvidenceResult<(Vec<u8>, usize, u64)> {
        let mut output = Vec::new();
        let (entries_written, bytes_written) = {
            let mut writer = NewcWriter::new(&mut output);
            let mut seen = BTreeSet::new();
            writer.add_absolute_tree(&mut seen, root)?;
            writer.finish(&mut seen)?;
            (writer.entries_written, writer.bytes_written)
        };
        Ok((output, entries_written, bytes_written))
    }

    #[test]
    fn newc_writer_records_regular_files_dirs_and_internal_symlinks_deterministically() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        let dir = root.join("dir");
        let file = dir.join("file.txt");
        let link = root.join("link.txt");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(&file, b"payload").expect("write file");
        unix_fs::symlink("dir/file.txt", &link).expect("symlink");

        let legacy = legacy_archive_tree(&root).expect("legacy archive");
        let (output, entries_written, bytes_written) = archive_tree(&root).expect("first archive");
        let (repeated, repeated_entries, repeated_bytes) =
            archive_tree(&root).expect("repeated archive");
        let archive = String::from_utf8_lossy(&output);

        assert_eq!(output, legacy);
        assert_eq!(output, repeated);
        assert_eq!(entries_written, repeated_entries);
        assert_eq!(bytes_written, repeated_bytes);
        assert!(archive.contains("root/dir/file.txt"));
        assert!(archive.contains("root/link.txt"));
        assert!(archive.contains(NEWC_TRAILER));
        assert!(entries_written >= 4);
        assert_eq!(bytes_written as usize, output.len());
    }

    #[test]
    fn newc_writer_rejects_symlink_that_escapes_the_observed_tree() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).expect("mkdir root");
        fs::write(&outside, b"outside").expect("write outside");
        unix_fs::symlink("../outside", root.join("escape")).expect("symlink");

        let error = archive_tree(&root).expect_err("escaping link rejected");

        assert!(error.message().contains("SymlinkTargetEscapes"));
    }

    #[test]
    fn newc_writer_rejects_tree_path_over_the_archive_bound() {
        const COMPONENT_BYTES: usize = 200;
        const COMPONENT_COUNT: usize = 3;
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        let component = "a".repeat(COMPONENT_BYTES);
        let mut deep = root.clone();
        for _ in 0..COMPONENT_COUNT {
            deep.push(&component);
        }
        fs::create_dir_all(&deep).expect("mkdir deep tree");
        fs::write(deep.join("payload"), b"payload").expect("write payload");

        let error = archive_tree(&root).expect_err("overlong path rejected");

        assert!(error.message().contains("PathBytes"));
    }

    #[test]
    fn newc_writer_rejects_output_past_the_local_archive_bound() {
        let mut output = Vec::new();
        let mut writer = NewcWriter::new(&mut output);
        writer.bytes_written = MAX_INITRD_ARCHIVE_BYTES;

        let error = writer
            .write_output(&[1])
            .expect_err("output bound rejected");

        assert!(error.message().contains("output byte bound"));
        assert!(output.is_empty());
    }

    #[test]
    fn closure_roots_reject_relative_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let list = temp.path().join("closure.txt");
        fs::write(&list, "relative/path\n").expect("write list");

        let err = read_closure_roots(&list).expect_err("relative rejected");

        assert!(err.message().contains("not absolute"));
    }
}
