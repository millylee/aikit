pub fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() < 9 {
        return "***".into();
    }
    let prefix = chars.iter().take(4).collect::<String>();
    let suffix = chars[chars.len() - 4..].iter().collect::<String>();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_hides_middle_section() {
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-1...cdef");
    }

    #[test]
    fn mask_secret_covers_short_values() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("short"), "***");
        assert_eq!(mask_secret("12345678"), "***");
    }

    #[test]
    fn mask_secret_keeps_minimal_length() {
        assert_eq!(mask_secret("123456789"), "1234...6789");
    }
}
