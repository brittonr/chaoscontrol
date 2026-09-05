use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;

const MAX_JSON_DEPTH: usize = 64;
const MAX_JSONL_STRUCTURAL_TOKENS: usize = 4_096;
const MAX_JSONL_STRING_BYTES: usize = 12 * 1024;

pub(crate) fn read_bounded_regular_file(
    path: &std::path::Path,
    maximum: usize,
) -> io::Result<String> {
    let mut file = open_regular_file(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > maximum as u64 {
        return Err(invalid_data("SDK JSONL exceeds the input byte limit"));
    }
    let read_limit = maximum
        .checked_add(1)
        .ok_or_else(|| invalid_data("SDK JSONL byte limit overflow"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(read_limit as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(invalid_data("SDK JSONL exceeds the input byte limit"));
    }
    String::from_utf8(bytes).map_err(|_| invalid_data("SDK JSONL is not UTF-8"))
}

fn open_regular_file(path: &std::path::Path) -> io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(invalid_data("SDK JSONL input is not a regular file"));
    }
    Ok(file)
}

pub(crate) fn preflight_json_line(input: &str) -> io::Result<()> {
    let mut stack = [0_u8; MAX_JSON_DEPTH];
    let mut depth = 0_usize;
    let mut tokens = 0_usize;
    let mut string_bytes = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in input.bytes() {
        if in_string {
            string_bytes = string_bytes
                .checked_add(1)
                .ok_or_else(|| invalid_data("JSON string byte count overflow"))?;
            if string_bytes > MAX_JSONL_STRING_BYTES {
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
                count_token(&mut tokens)?;
                if depth == MAX_JSON_DEPTH {
                    return Err(invalid_data("JSON nesting depth exceeded"));
                }
                stack[depth] = byte;
                depth += 1;
            }
            b'}' | b']' => {
                count_token(&mut tokens)?;
                if depth == 0 {
                    return Err(invalid_data("JSON has an unmatched closing token"));
                }
                depth -= 1;
                let expected = if byte == b'}' { b'{' } else { b'[' };
                if stack[depth] != expected {
                    return Err(invalid_data("JSON structural tokens are mismatched"));
                }
            }
            b',' | b':' => count_token(&mut tokens)?,
            _ => {}
        }
    }
    if in_string || escaped || depth != 0 {
        return Err(invalid_data("JSON lexical structure is incomplete"));
    }
    Ok(())
}

fn count_token(count: &mut usize) -> io::Result<()> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| invalid_data("JSON structural token count overflow"))?;
    if *count > MAX_JSONL_STRUCTURAL_TOKENS {
        return Err(invalid_data("JSON structural token budget exceeded"));
    }
    Ok(())
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
