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

/// Escape table-cell delimiters so content cannot rewrite the table shape.
pub fn escape_markdown_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\r' | '\n' => escaped.push(' '),
            '\\' => escaped.push_str("\\\\"),
            '|' => escaped.push_str("\\|"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Replace line breaks with spaces for single-line Markdown fields.
pub fn flatten_lines(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect()
}

/// Wrap content in a fenced code block that cannot be closed by its content.
pub fn markdown_code_block(content: &str) -> String {
    let mut fence_length = 3_usize;
    while content.contains(&"`".repeat(fence_length)) {
        fence_length += 1;
    }
    let fence = "`".repeat(fence_length);
    format!("{fence}\n{content}\n{fence}\n\n")
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

    #[test]
    fn markdown_helpers_escape_delimiters_and_fences() {
        assert_eq!(escape_markdown_cell("a|b\\c\nnext"), "a\\|b\\\\c next");
        let fenced = markdown_code_block("before\n```\nafter");
        assert!(fenced.starts_with("````\n"));
        assert!(fenced.ends_with("\n````\n\n"));
        assert_eq!(flatten_lines("a\r\nb\nc"), "a  b c");
    }
}
