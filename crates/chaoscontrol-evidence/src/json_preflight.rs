const MAX_JSON_DEPTH: usize = 64;
const MAX_JSONL_STRUCTURAL_TOKENS: usize = 4_096;
const MAX_JSONL_STRING_BYTES: usize = 12 * 1024;
const MAX_REPORT_STRUCTURAL_TOKENS_PER_ENTRY: usize = 96;
const MAX_REPORT_BASE_STRUCTURAL_TOKENS: usize = 1_024;
const MAX_REPORT_STRUCTURAL_TOKENS: usize =
    chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES
        * MAX_REPORT_STRUCTURAL_TOKENS_PER_ENTRY
        + MAX_REPORT_BASE_STRUCTURAL_TOKENS;
const MAX_REPORT_STRING_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub(crate) struct JsonLimits {
    maximum_depth: usize,
    maximum_structural_tokens: usize,
    maximum_string_bytes: usize,
}

pub(crate) const JSONL_LINE_LIMITS: JsonLimits = JsonLimits {
    maximum_depth: MAX_JSON_DEPTH,
    maximum_structural_tokens: MAX_JSONL_STRUCTURAL_TOKENS,
    maximum_string_bytes: MAX_JSONL_STRING_BYTES,
};

pub(crate) const QUALITY_REPORT_LIMITS: JsonLimits = JsonLimits {
    maximum_depth: MAX_JSON_DEPTH,
    maximum_structural_tokens: MAX_REPORT_STRUCTURAL_TOKENS,
    maximum_string_bytes: MAX_REPORT_STRING_BYTES,
};

pub(crate) fn preflight_json(input: &str, limits: JsonLimits) -> crate::EvidenceResult<()> {
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
                .ok_or_else(|| crate::EvidenceError::new("JSON string byte count overflow"))?;
            if string_bytes > limits.maximum_string_bytes {
                return Err(crate::EvidenceError::new(
                    "JSON string byte budget exceeded",
                ));
            }
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            } else if byte < b' ' {
                return Err(crate::EvidenceError::new(
                    "JSON string contains a control byte",
                ));
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                count_structural(&mut structural_tokens, limits)?;
                if depth >= limits.maximum_depth {
                    return Err(crate::EvidenceError::new("JSON nesting depth exceeded"));
                }
                stack[depth] = byte;
                depth += 1;
            }
            b'}' | b']' => {
                count_structural(&mut structural_tokens, limits)?;
                if depth == 0 {
                    return Err(crate::EvidenceError::new(
                        "JSON has an unmatched closing token",
                    ));
                }
                depth -= 1;
                let expected = if byte == b'}' { b'{' } else { b'[' };
                if stack[depth] != expected {
                    return Err(crate::EvidenceError::new(
                        "JSON structural tokens are mismatched",
                    ));
                }
            }
            b',' | b':' => count_structural(&mut structural_tokens, limits)?,
            _ => {}
        }
    }
    if in_string || escaped || depth != 0 {
        return Err(crate::EvidenceError::new(
            "JSON lexical structure is incomplete",
        ));
    }
    Ok(())
}

fn count_structural(count: &mut usize, limits: JsonLimits) -> crate::EvidenceResult<()> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| crate::EvidenceError::new("JSON structural token count overflow"))?;
    if *count > limits.maximum_structural_tokens {
        return Err(crate::EvidenceError::new(
            "JSON structural token budget exceeded",
        ));
    }
    Ok(())
}
