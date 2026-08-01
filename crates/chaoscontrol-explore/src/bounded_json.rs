use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_STRUCTURAL_TOKENS: usize = 500_000;
const MAX_JSON_STRING_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn read_checkpoint(path: &Path) -> io::Result<String> {
    let mut file = open_regular_file(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > MAX_CHECKPOINT_BYTES {
        return Err(invalid_data("checkpoint exceeds the input byte limit"));
    }
    let read_limit = MAX_CHECKPOINT_BYTES
        .checked_add(1)
        .ok_or_else(|| invalid_data("checkpoint read limit overflow"))?;
    let mut input = String::new();
    file.by_ref().take(read_limit).read_to_string(&mut input)?;
    if input.len() as u64 > MAX_CHECKPOINT_BYTES {
        return Err(invalid_data("checkpoint exceeds the input byte limit"));
    }
    preflight_json(&input)?;
    Ok(input)
}

fn open_regular_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(invalid_data("checkpoint input is not a regular file"));
    }
    Ok(file)
}

fn preflight_json(input: &str) -> io::Result<()> {
    let mut stack = [0_u8; MAX_JSON_DEPTH];
    let mut depth = 0_usize;
    let mut structural_tokens = 0_usize;
    let mut string_bytes = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in input.bytes() {
        if in_string {
            string_bytes = string_bytes
                .checked_add(1)
                .ok_or_else(|| invalid_data("JSON string byte count overflow"))?;
            if string_bytes > MAX_JSON_STRING_BYTES {
                return Err(invalid_data("JSON string byte budget exceeded"));
            }
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            } else if byte < b' ' {
                return Err(invalid_data("JSON string contains a control byte"));
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                count_structural(&mut structural_tokens)?;
                if depth >= MAX_JSON_DEPTH {
                    return Err(invalid_data("JSON nesting depth exceeded"));
                }
                stack[depth] = byte;
                depth += 1;
            }
            b'}' | b']' => {
                count_structural(&mut structural_tokens)?;
                if depth == 0 {
                    return Err(invalid_data("JSON has an unmatched closing token"));
                }
                depth -= 1;
                let expected = if byte == b'}' { b'{' } else { b'[' };
                if stack[depth] != expected {
                    return Err(invalid_data("JSON structural tokens are mismatched"));
                }
            }
            b',' | b':' => count_structural(&mut structural_tokens)?,
            _ => {}
        }
    }
    if in_string || escaped || depth != 0 {
        return Err(invalid_data("JSON lexical structure is incomplete"));
    }
    Ok(())
}

fn count_structural(count: &mut usize) -> io::Result<()> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| invalid_data("JSON structural token count overflow"))?;
    if *count > MAX_JSON_STRUCTURAL_TOKENS {
        return Err(invalid_data("JSON structural token budget exceeded"));
    }
    Ok(())
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn reads_a_bounded_regular_json_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        std::fs::write(&path, r#"{"bugs":[]}"#).expect("fixture writes");

        assert_eq!(
            read_checkpoint(&path).expect("fixture reads"),
            r#"{"bugs":[]}"#
        );
    }

    #[test]
    fn rejects_symlink_and_incomplete_json_inputs() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target.json");
        let link = directory.path().join("link.json");
        std::fs::write(&target, "{}").expect("target writes");
        symlink(&target, &link).expect("symlink creates");

        assert!(read_checkpoint(&link).is_err());
        assert!(preflight_json(r#"{"bugs":["#).is_err());
    }
}
