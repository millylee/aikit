use std::{
    fs,
    path::{Path, PathBuf},
};

use aikit_core::Result;

const TOKEN_BYTES: usize = 32;

pub fn token_path(aikit_dir: &Path) -> PathBuf {
    aikit_dir.join("daemon.token")
}

pub fn generate_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

pub fn validate_token_format(token: &str) -> bool {
    token.len() == TOKEN_BYTES * 2 && token.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn ensure_token(aikit_dir: &Path) -> Result<String> {
    let path = token_path(aikit_dir);
    if let Some(existing) = fs::read_to_string(&path)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|token| validate_token_format(token))
    {
        return Ok(existing);
    }
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
        assert!(validate_token_format(&generate_token()));
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
