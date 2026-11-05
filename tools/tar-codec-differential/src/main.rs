//! Differential evaluation of the `tar` and `tar-codec` archive parsers.
//!
//! This harness is not product code. It lists the same archives with both
//! parsers, compares entry lists, applies the OCI intake admission rules to
//! each list, and measures listing time. Any disagreement on the adversarial
//! corpus is reported as evidence; the recommendation is documented in
//! `docs/tar-codec-evaluation.md`.

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::{Component, PathBuf};
use std::time::Instant;

use tar::{Builder, EntryType, Header};
use tar_codec::{Archive as _, TarArchive};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    path: String,
    kind: String,
    size: u64,
    link: Option<String>,
}

trait TarLister {
    fn name(&self) -> &'static str;
    fn list(&self, bytes: &[u8]) -> Result<Vec<Entry>, String>;
}

struct TarCrateLister;

impl TarLister for TarCrateLister {
    fn name(&self) -> &'static str {
        "tar-0.4"
    }

    fn list(&self, bytes: &[u8]) -> Result<Vec<Entry>, String> {
        let mut archive = tar::Archive::new(Cursor::new(bytes));
        let mut entries = Vec::new();
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?.display().to_string();
            let header = entry.header();
            let kind = match header.entry_type() {
                EntryType::Regular => "file",
                EntryType::Directory => "dir",
                EntryType::Symlink => "symlink",
                EntryType::Link => "hardlink",
                _ => "special",
            }
            .to_string();
            let link = entry
                .link_name()
                .map_err(|e| e.to_string())?
                .map(|target| target.display().to_string());
            entries.push(Entry {
                path,
                kind,
                size: entry.size(),
                link,
            });
        }
        Ok(entries)
    }
}

struct AstralLister;

impl TarLister for AstralLister {
    fn name(&self) -> &'static str {
        "tar-codec-0.0.16"
    }

    fn list(&self, bytes: &[u8]) -> Result<Vec<Entry>, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime.block_on(async {
            let mut members = TarArchive::new(Cursor::new(bytes)).members();
            let mut entries = Vec::new();
            loop {
                let member = members.next().await.map_err(|e| e.to_string())?;
                let Some(member) = member else { break };
                let entry = match member {
                    tar_codec::Member::File { metadata, size, .. } => Entry {
                        path: metadata.path,
                        kind: "file".to_string(),
                        size,
                        link: None,
                    },
                    tar_codec::Member::Directory { metadata } => Entry {
                        path: metadata.path,
                        kind: "dir".to_string(),
                        size: 0,
                        link: None,
                    },
                    tar_codec::Member::SymbolicLink { metadata, target } => Entry {
                        path: metadata.path,
                        kind: "symlink".to_string(),
                        size: 0,
                        link: Some(target),
                    },
                    tar_codec::Member::HardLink {
                        metadata, target, ..
                    } => Entry {
                        path: metadata.path,
                        kind: "hardlink".to_string(),
                        size: 0,
                        link: Some(target),
                    },
                    tar_codec::Member::Special { metadata, .. } => Entry {
                        path: metadata.path,
                        kind: "special".to_string(),
                        size: 0,
                        link: None,
                    },
                };
                entries.push(entry);
            }
            Ok(entries)
        })
    }
}

/// Admission mirror of the OCI intake rules: relative normal path, file or
/// directory (whiteouts are regular files), size bounded by the caller.
fn admits(entry: &Entry) -> bool {
    let path = PathBuf::from(&entry.path);
    if path.as_os_str().is_empty() {
        return false;
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::CurDir => {}
            _ => return false,
        }
    }
    !relative.as_os_str().is_empty() && matches!(entry.kind.as_str(), "file" | "dir")
}

fn raw_header(name: &str, typeflag: u8, size: u64, link: &str, prefix: &str) -> [u8; 512] {
    let mut header = [0_u8; 512];
    let name_bytes = name.as_bytes();
    header[..name_bytes.len().min(100)].copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_field = format!("{size:011o}\0");
    header[124..136].copy_from_slice(size_field.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = typeflag;
    let link_bytes = link.as_bytes();
    header[157..157 + link_bytes.len().min(100)]
        .copy_from_slice(&link_bytes[..link_bytes.len().min(100)]);
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let prefix_bytes = prefix.as_bytes();
    if !prefix_bytes.is_empty() {
        header[345..345 + prefix_bytes.len().min(155)]
            .copy_from_slice(&prefix_bytes[..prefix_bytes.len().min(155)]);
    }
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    let field = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(field.as_bytes());
    header
}

fn raw_archive(entries: &[([u8; 512], &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (header, payload) in entries {
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(payload);
        let padding = (512 - payload.len() % 512) % 512;
        bytes.extend(std::iter::repeat(0).take(padding));
    }
    bytes.extend(std::iter::repeat(0).take(1024));
    bytes
}

fn pax_record(path: &str) -> Vec<u8> {
    let body = format!(" path={path}\n");
    let mut total = body.len() + 1;
    loop {
        let digits = total.to_string().len();
        let next = body.len() + digits;
        if next == total {
            return format!("{total}{body}").into_bytes();
        }
        total = next;
    }
}

fn push_file<W: std::io::Write>(builder: &mut Builder<W>, path: &str, bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .expect("append file");
}

fn benign_corpus() -> Vec<(&'static str, Vec<u8>)> {
    let mut short = Vec::new();
    {
        let mut builder = Builder::new(&mut short);
        push_file(&mut builder, "etc/issue", b"chaos\n");
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "etc/", &[][..])
            .expect("append dir");
        builder.finish().expect("finish");
    }

    let mut long = Vec::new();
    {
        let long_name = format!("usr/share/{}/file.txt", "a".repeat(120));
        let mut builder = Builder::new(&mut long);
        push_file(&mut builder, &long_name, b"long\n");
        builder.finish().expect("finish");
    }

    let mut whiteout = Vec::new();
    {
        let mut builder = Builder::new(&mut whiteout);
        push_file(&mut builder, "var/lib/.wh.cached", b"");
        push_file(&mut builder, "var/lib/.wh..wh..opq", b"");
        builder.finish().expect("finish");
    }

    vec![
        ("short", short),
        ("long-name", long),
        ("whiteouts", whiteout),
    ]
}

fn adversarial_corpus() -> Vec<(&'static str, Vec<u8>)> {
    let mut corpus = Vec::new();

    corpus.push((
        "absolute-path",
        raw_archive(&[(raw_header("/etc/passwd", b'0', 4, "", ""), b"evil")]),
    ));
    corpus.push((
        "parent-traversal",
        raw_archive(&[(raw_header("../escape", b'0', 4, "", ""), b"evil")]),
    ));
    corpus.push((
        "duplicate-entry",
        raw_archive(&[
            (raw_header("etc/issue", b'0', 3, "", ""), b"one"),
            (raw_header("etc/issue", b'0', 3, "", ""), b"two"),
        ]),
    ));
    corpus.push((
        "symlink",
        raw_archive(&[(raw_header("link", b'2', 0, "/etc/passwd", ""), b"")]),
    ));
    corpus.push((
        "hardlink",
        raw_archive(&[(raw_header("hard", b'1', 0, "../outside", ""), b"")]),
    ));
    corpus.push((
        "device-node",
        raw_archive(&[(raw_header("dev/null", b'3', 0, "", ""), b"")]),
    ));
    let mut truncated = raw_archive(&[(raw_header("etc/issue", b'0', 512, "", ""), &[0u8; 512])]);
    truncated.truncate(700);
    corpus.push(("truncated", truncated));

    let pax_conflict = {
        let pax = pax_record("pax/path.txt");
        let mut pax_header = raw_header("PaxHeader", b'x', pax.len() as u64, "", "");
        pax_header[148..156].fill(b' ');
        let checksum: u32 = pax_header.iter().map(|b| u32::from(*b)).sum();
        pax_header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        let mut file_header = raw_header("ustar-path.txt", b'0', 4, "", "");
        file_header[148..156].fill(b' ');
        let checksum: u32 = file_header.iter().map(|b| u32::from(*b)).sum();
        file_header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        raw_archive(&[(pax_header, pax.as_slice()), (file_header, b"pax\n")])
    };
    corpus.push(("pax-overrides-name", pax_conflict));

    let mut gnu_longname = Vec::new();
    {
        let mut builder = Builder::new(&mut gnu_longname);
        let name = format!("gnu/{}/escape.txt", "b".repeat(130));
        push_file(&mut builder, &name, b"gnu\n");
        builder.finish().expect("finish");
    }
    corpus.push(("gnu-longname", gnu_longname));

    let mixed = {
        let pax = pax_record("mixed/pax.txt");
        let pax_header = raw_header("PaxHeader", b'x', pax.len() as u64, "", "");
        let ustar_header = raw_header("mixed/ustar.txt", b'0', 4, "", "");
        raw_archive(&[(pax_header, pax.as_slice()), (ustar_header, b"mix\n")])
    };
    corpus.push(("pax-then-ustar", mixed));

    corpus
}

fn main() {
    let listers: [&dyn TarLister; 2] = [&TarCrateLister, &AstralLister];
    let mut failures = 0;

    println!("benign corpus: entry lists must match");
    for (name, bytes) in benign_corpus() {
        let lists: Vec<Result<Vec<Entry>, String>> =
            listers.iter().map(|lister| lister.list(&bytes)).collect();
        match (&lists[0], &lists[1]) {
            (Ok(left), Ok(right)) if left == right => {
                println!("  {name}: identical ({} entries)", left.len());
            }
            (left, right) => {
                failures += 1;
                println!(
                    "  {name}: DIFFER\n    tar={left:?}\n    astral={right:?}"
                );
            }
        }
    }

    println!("adversarial corpus: disagreements are recorded");
    let mut disagreements = Vec::new();
    for (name, bytes) in adversarial_corpus() {
        let mut line = String::new();
        let mut admitted = Vec::new();
        let mut results = Vec::new();
        for lister in listers {
            let result = lister.list(&bytes);
            let admitted_count = result
                .as_ref()
                .map(|entries| entries.iter().filter(|entry| admits(entry)).count())
                .unwrap_or(0);
            admitted.push(admitted_count);
            results.push(result);
        }
        let _ = write!(
            line,
            "  {name}: tar={} astral={}",
            describe(&results[0], admitted[0]),
            describe(&results[1], admitted[1])
        );
        let agreement = match (&results[0], &results[1]) {
            (Ok(left), Ok(right)) => left == right,
            (Err(_), Err(_)) => true,
            _ => false,
        };
        let diverges = !agreement;
        if diverges {
            line.push_str(" [divergence recorded: switch would fail closed]");
            disagreements.push(name);
        }
        if results[0].is_ok() && results[1].is_ok() && admitted.iter().any(|count| *count > 0) {
            line.push_str(" [admitted by at least one parser]");
        }
        println!("{line}");
    }

    println!("benchmark: listing a synthesized layer");
    let mut layer = Vec::new();
    {
        let mut builder = Builder::new(&mut layer);
        for index in 0..200 {
            let path = format!("usr/share/workload/file-{index:03}.txt");
            push_file(&mut builder, &path, &[b'x'; 4096]);
        }
        builder.finish().expect("finish");
    }
    let iterations = 30;
    for lister in listers {
        let start = Instant::now();
        for _ in 0..iterations {
            lister.list(&layer).expect("benchmark listing");
        }
        let elapsed = start.elapsed();
        println!(
            "  {}: layer={} KiB iterations={} total={:?} per-run={:?}",
            lister.name(),
            layer.len() / 1024,
            iterations,
            elapsed,
            elapsed / iterations
        );
    }

    println!(
        "divergences on adversarial corpus: {} ({})",
        disagreements.len(),
        if disagreements.is_empty() {
            "none".to_string()
        } else {
            disagreements.join(", ")
        }
    );
    if failures > 0 {
        eprintln!("benign corpus differed in {failures} case(s)");
        std::process::exit(1);
    }
}

fn describe(result: &Result<Vec<Entry>, String>, admitted: usize) -> String {
    match result {
        Ok(entries) => format!("Ok({} entries, {admitted} admitted)", entries.len()),
        Err(error) => format!("Err({error})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists(bytes: &[u8]) -> (Result<Vec<Entry>, String>, Result<Vec<Entry>, String>) {
        (
            TarCrateLister.list(bytes),
            AstralLister.list(bytes),
        )
    }

    #[test]
    fn benign_corpora_match() {
        for (name, bytes) in benign_corpus() {
            let (left, right) = lists(&bytes);
            assert_eq!(left, right, "benign case {name} differs between parsers");
        }
    }

    #[test]
    fn adversarial_corpora_agree_and_unsafe_entries_are_rejected() {
        for (name, bytes) in adversarial_corpus() {
            let (left, right) = lists(&bytes);
            let agreement = match (&left, &right) {
                (Ok(a), Ok(b)) => a == b,
                (Err(_), Err(_)) => true,
                _ => false,
            };
            assert!(agreement, "adversarial case {name} disagrees: {left:?} vs {right:?}");
        }

        for unsafe_case in [
            "absolute-path",
            "parent-traversal",
            "symlink",
            "hardlink",
            "device-node",
        ] {
            let (_, bytes) = adversarial_corpus()
                .into_iter()
                .find(|(name, _)| *name == unsafe_case)
                .expect("adversarial case");
            let (left, right) = lists(&bytes);
            for entries in [left.expect("parsed"), right.expect("parsed")] {
                assert!(
                    entries.iter().all(|entry| !admits(entry)),
                    "{unsafe_case} admitted an unsafe entry"
                );
            }
        }
    }

    #[test]
    fn truncated_archive_is_rejected_by_both() {
        let (_, bytes) = adversarial_corpus()
            .into_iter()
            .find(|(name, _)| *name == "truncated")
            .expect("truncated case");
        let (left, right) = lists(&bytes);
        assert!(left.is_err() && right.is_err(), "truncated archive parsed");
    }
}
