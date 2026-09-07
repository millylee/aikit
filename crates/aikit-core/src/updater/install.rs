use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use crate::{AikitError, Result};

#[derive(Debug)]
pub struct PreparedBinaryInstallation {
    target: PathBuf,
    candidate: tempfile::TempPath,
    lock: File,
}

impl PreparedBinaryInstallation {
    pub fn target_path(&self) -> &Path {
        &self.target
    }

    pub fn candidate_path(&self) -> &Path {
        &self.candidate
    }

    pub fn commit(self) -> Result<BinaryInstallation> {
        let backup = if self.target.exists() {
            if !self.target.is_file() {
                return Err(AikitError::Provider(format!(
                    "invalid update destination: {}",
                    self.target.display()
                )));
            }
            cleanup_previous_binary_unlocked(&self.target)?;
            let backup = sibling_path(&self.target, ".aikit-backup")?;
            fs::rename(&self.target, &backup)?;
            Some(backup)
        } else {
            None
        };

        if let Err(err) = self.candidate.persist_noclobber(&self.target) {
            if let Some(backup) = &backup {
                if let Err(rollback_error) = fs::rename(backup, &self.target) {
                    return Err(AikitError::Provider(format!(
                        "update replacement failed: {}; rollback failed: {rollback_error}; backup: {}",
                        err.error,
                        backup.display()
                    )));
                }
            }
            return Err(AikitError::Io(err.error));
        }

        Ok(BinaryInstallation {
            target: self.target,
            backup,
            _lock: self.lock,
        })
    }
}

#[derive(Debug)]
pub struct BinaryInstallation {
    target: PathBuf,
    backup: Option<PathBuf>,
    _lock: File,
}

impl BinaryInstallation {
    pub fn backup_path(&self) -> Option<&Path> {
        self.backup.as_deref()
    }

    pub fn target_path(&self) -> &Path {
        &self.target
    }

    pub fn rollback(self) -> Result<()> {
        match self.backup {
            Some(backup) => fs::rename(backup, self.target)?,
            None => fs::remove_file(self.target)?,
        }
        Ok(())
    }
}

pub fn install_binary(staged: &Path, target: &Path) -> Result<()> {
    install_binary_with_backup(staged, target)?;
    Ok(())
}

pub fn install_binary_with_backup(staged: &Path, target: &Path) -> Result<BinaryInstallation> {
    prepare_binary_installation(staged, target)?.commit()
}

pub fn prepare_binary_installation(
    staged: &Path,
    target: &Path,
) -> Result<PreparedBinaryInstallation> {
    if !staged.is_file() {
        return Err(AikitError::Provider(format!(
            "update candidate is not a file: {}",
            staged.display()
        )));
    }
    if target.exists() && (!target.is_file() || staged.canonicalize()? == target.canonicalize()?) {
        return Err(AikitError::Provider(format!(
            "invalid update destination: {}",
            target.display()
        )));
    }
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let lock = lock_binary_installation(target)?;

    let mut candidate = tempfile::Builder::new()
        .prefix(".aikit-update-")
        .tempfile_in(parent)?;
    io::copy(&mut File::open(staged)?, candidate.as_file_mut())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        candidate
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    candidate.as_file().sync_all()?;

    Ok(PreparedBinaryInstallation {
        target: target.to_path_buf(),
        candidate: candidate.into_temp_path(),
        lock,
    })
}

pub fn cleanup_previous_binary(target: &Path) -> Result<()> {
    let _lock = match lock_binary_installation(target) {
        Ok(lock) => lock,
        Err(AikitError::Io(err)) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    cleanup_previous_binary_unlocked(target)
}

fn lock_binary_installation(target: &Path) -> Result<File> {
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(sibling_path(target, ".aikit-update.lock")?)?;
    lock.try_lock()
        .map_err(|err| AikitError::Provider(format!("cannot lock executable for update: {err}")))?;
    Ok(lock)
}

fn cleanup_previous_binary_unlocked(target: &Path) -> Result<()> {
    match fs::remove_file(sibling_path(target, ".aikit-backup")?) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AikitError::Io(err)),
    }
}

fn sibling_path(target: &Path, suffix: &str) -> Result<PathBuf> {
    let mut name = target
        .file_name()
        .ok_or_else(|| AikitError::Provider("executable path has no filename".into()))?
        .to_os_string();
    name.push(suffix);
    Ok(target.with_file_name(name))
}
