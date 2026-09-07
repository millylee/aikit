use std::{
    fs,
    path::{Path, PathBuf},
};

use aikit_core::Result;

const TOKEN_LEN: usize = 12;
const TOKEN_ALPHABET: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!@#$%^&*()-_=+";
const LEGACY_TOKEN_HEX_LEN: usize = 64;

pub fn token_path(aikit_dir: &Path) -> PathBuf {
    aikit_dir.join("daemon.token")
}

pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..TOKEN_LEN)
        .map(|_| TOKEN_ALPHABET[rng.random_range(0..TOKEN_ALPHABET.len())] as char)
        .collect()
}

pub fn validate_token_format(token: &str) -> bool {
    is_legacy_token_format(token)
        || (token.len() == TOKEN_LEN && token.bytes().all(|byte| TOKEN_ALPHABET.contains(&byte)))
}

fn is_legacy_token_format(token: &str) -> bool {
    token.len() == LEGACY_TOKEN_HEX_LEN && token.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn read_valid_token(aikit_dir: &Path) -> Option<String> {
    fs::read_to_string(token_path(aikit_dir))
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|token| validate_token_format(token))
}

pub fn ensure_token(aikit_dir: &Path) -> Result<String> {
    if let Some(existing) = read_valid_token(aikit_dir) {
        return Ok(existing);
    }
    let path = token_path(aikit_dir);
    let token = generate_token();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &token)?;
    set_owner_only(&path)?;
    Ok(token)
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_has_expected_format() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_LEN);
        assert!(validate_token_format(&token));
        assert!(token.bytes().any(|byte| byte.is_ascii_digit()));
        assert!(token.bytes().any(|byte| byte.is_ascii_alphabetic()));
    }

    #[test]
    fn legacy_hex_tokens_still_validate() {
        assert!(validate_token_format(&"a".repeat(LEGACY_TOKEN_HEX_LEN)));
    }

    #[test]
    fn ensure_token_creates_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let first = ensure_token(dir.path()).unwrap();
        let second = ensure_token(dir.path()).unwrap();
        assert_eq!(first, second);
        assert!(validate_token_format(&first));
    }

    #[test]
    fn ensure_token_regenerates_when_file_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = token_path(dir.path());
        std::fs::write(&path, "not-a-token").unwrap();
        let token = ensure_token(dir.path()).unwrap();
        assert!(validate_token_format(&token));
        assert_ne!(token, "not-a-token");
    }
}
