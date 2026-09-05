use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;

pub(crate) fn read_bounded_regular_file(
    path: &std::path::Path,
    maximum_bytes: u64,
) -> crate::EvidenceResult<String> {
    let bytes = read_bounded_regular_bytes(path, maximum_bytes)?;
    String::from_utf8(bytes).map_err(|error| {
        crate::EvidenceError::new(format!("{}: invalid UTF-8: {error}", path.display()))
    })
}

pub(crate) fn read_bounded_regular_bytes(
    path: &std::path::Path,
    maximum_bytes: u64,
) -> crate::EvidenceResult<Vec<u8>> {
    let mut file = open_regular_file(path)?;
    let metadata = file
        .metadata()
        .map_err(|error| crate::EvidenceError::new(format!("{}: {error}", path.display())))?;
    if metadata.len() > maximum_bytes {
        return Err(crate::EvidenceError::new(format!(
            "{}: file exceeds the input byte limit",
            path.display()
        )));
    }
    let bounded_length = maximum_bytes
        .checked_add(1)
        .ok_or_else(|| crate::EvidenceError::new("input byte limit overflow"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(bounded_length)
        .read_to_end(&mut bytes)
        .map_err(|error| crate::EvidenceError::new(format!("{}: {error}", path.display())))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(crate::EvidenceError::new(format!(
            "{}: file exceeds the input byte limit",
            path.display()
        )));
    }
    Ok(bytes)
}

fn open_regular_file(path: &std::path::Path) -> crate::EvidenceResult<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| crate::EvidenceError::new(format!("{}: {error}", path.display())))?;
    let file_type = file
        .metadata()
        .map_err(|error| crate::EvidenceError::new(format!("{}: {error}", path.display())))?
        .file_type();
    if !file_type.is_file() {
        return Err(crate::EvidenceError::new(format!(
            "{}: expected a regular file",
            path.display()
        )));
    }
    Ok(file)
}
