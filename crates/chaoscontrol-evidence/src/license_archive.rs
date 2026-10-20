//! Source-archive license checks for repository-owned packages.
//!
//! The filesystem license rail checks manifests and text files in the working
//! tree. A produced source archive is a separate distribution surface: it can
//! omit a license text or retain stale metadata even when the working tree is
//! correct. This module validates the archive bytes themselves.
// r[impl chaoscontrol.unified_agpl.metadata]
// r[impl chaoscontrol.unified_agpl.validation]

use std::io::{Cursor, Read};
use std::path::Path;

use tar::Archive;

/// Validate one uncompressed source archive against the expected package
/// license. The archive must carry `Cargo.toml` and `LICENSE` at its root.
pub fn validate_package_archive(bytes: &[u8], expected_license: &str) -> Result<(), String> {
    let mut archive = Archive::new(Cursor::new(bytes));
    let mut manifest = None;
    let mut license = None;
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry
            .path()
            .map_err(|error| error.to_string())?
            .into_owned();
        if path == Path::new("Cargo.toml") {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|error| error.to_string())?;
            manifest = Some(text);
        } else if path == Path::new("LICENSE") {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|error| error.to_string())?;
            license = Some(text);
        }
    }
    let manifest = manifest.ok_or("source archive is missing Cargo.toml")?;
    let license = license.ok_or("source archive is missing LICENSE")?;
    let declared = manifest
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("license = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .ok_or("source archive Cargo.toml has no explicit license")?;
    if declared != expected_license {
        return Err(format!(
            "source archive declares {declared:?}, expected {expected_license:?}"
        ));
    }
    if expected_license == "AGPL-3.0-or-later"
        && !license.contains("GNU AFFERO GENERAL PUBLIC LICENSE")
    {
        return Err("source archive LICENSE is not the AGPL text".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tar::{Builder, Header};

    fn archive(manifest: &str, license: Option<&str>) -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());
        let mut header = Header::new_gnu();
        header.set_size(manifest.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "Cargo.toml", manifest.as_bytes())
            .unwrap();
        if let Some(license) = license {
            let mut header = Header::new_gnu();
            header.set_size(license.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "LICENSE", license.as_bytes())
                .unwrap();
        }
        builder.into_inner().unwrap()
    }

    // r[verify chaoscontrol.unified_agpl.metadata]
    // r[verify chaoscontrol.unified_agpl.validation]
    #[test]
    fn complete_agpl_archive_is_accepted_and_stale_archives_fail() {
        let agpl = "[package]\nname = \"owned\"\nlicense = \"AGPL-3.0-or-later\"\n";
        let text = "GNU AFFERO GENERAL PUBLIC LICENSE\nVersion 3";
        assert!(validate_package_archive(&archive(agpl, Some(text)), "AGPL-3.0-or-later").is_ok());
        assert!(validate_package_archive(&archive(agpl, None), "AGPL-3.0-or-later").is_err());
        assert!(validate_package_archive(
            &archive(agpl, Some("Apache License")),
            "AGPL-3.0-or-later"
        )
        .is_err());
        let stale = "[package]\nname = \"owned\"\nlicense = \"Apache-2.0\"\n";
        assert!(
            validate_package_archive(&archive(stale, Some(text)), "AGPL-3.0-or-later").is_err()
        );
        let workspace = "[package]\nname = \"owned\"\nlicense.workspace = true\n";
        assert!(
            validate_package_archive(&archive(workspace, Some(text)), "AGPL-3.0-or-later").is_err()
        );
    }
}
