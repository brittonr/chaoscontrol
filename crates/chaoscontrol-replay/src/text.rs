//! Byte-bounded string slices that preserve UTF-8 boundaries.

/// Return the longest prefix of `value` no longer than `max_bytes`.
pub fn prefix_at_char_boundary(value: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Return a suffix of `value` no longer than `max_bytes`.
pub fn suffix_at_char_boundary(value: &str, max_bytes: usize) -> &str {
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_remain_on_char_boundaries() {
        let value = "a\u{e9}\u{e9}\u{e9}";
        let prefix = prefix_at_char_boundary(value, 2);
        assert_eq!(prefix, "a");
        assert!(value.is_char_boundary(prefix.len()));

        let suffix = suffix_at_char_boundary(value, 3);
        assert_eq!(suffix, "\u{e9}");
        assert!(value.is_char_boundary(value.len() - suffix.len()));

        assert_eq!(prefix_at_char_boundary(value, 0), "");
        assert_eq!(suffix_at_char_boundary(value, 0), "");
        assert_eq!(prefix_at_char_boundary(value, value.len()), value);
        assert_eq!(suffix_at_char_boundary(value, value.len()), value);
    }
}
