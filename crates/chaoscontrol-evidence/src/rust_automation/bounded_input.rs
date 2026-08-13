pub fn validate_byte_length(label: &str, actual: u64, maximum: u64) -> Result<(), String> {
    assert!(!label.is_empty(), "input label must not be empty");
    assert!(maximum > 0, "maximum input bytes must be positive");
    if actual > maximum {
        return Err(format!("{label}: byte bound exceeded"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_byte_length;

    const MAXIMUM: u64 = 16;

    #[test]
    fn accepts_input_at_bound() {
        assert_eq!(validate_byte_length("receipt", MAXIMUM, MAXIMUM), Ok(()));
    }

    #[test]
    fn rejects_input_over_bound() {
        assert_eq!(
            validate_byte_length("receipt", MAXIMUM + 1, MAXIMUM),
            Err(String::from("receipt: byte bound exceeded"))
        );
    }
}
