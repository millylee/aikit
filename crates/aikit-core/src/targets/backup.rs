use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde::Serialize;
use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};

use crate::{AikitError, Result};

const BACKUP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year][month][day]-[hour][minute][second].[subsecond digits:3]");
const LOG_TIME_FORMAT: &[FormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
const DEFAULT_RETENTION_PER_TARGET: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct BackupLogRecord {
    pub target_id: String,
    pub source_path: PathBuf,
    pub backup_path: PathBuf,
    pub written_at: String,
    pub status: String,
}

pub fn backup_file(target_id: &str, path: &Path) -> Result<Option<PathBuf>> {
    backup_file_to_root(target_id, path, &default_aikit_dir()?)
}

pub fn backup_file_to_root(
    target_id: &str,
    path: &Path,
    aikit_dir: &Path,
) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(BACKUP_FORMAT)
        .map_err(|err| AikitError::TargetWrite(format!("failed to format backup time: {err}")))?;
    let backup_dir = aikit_dir.join("backups").join(sanitize_segment(target_id));
    fs::create_dir_all(&backup_dir)?;
    let backup_path = unique_backup_path(&backup_dir, &timestamp, path);
    fs::copy(path, &backup_path)?;
    append_backup_log(
        aikit_dir,
        BackupLogRecord {
            target_id: target_id.to_string(),
            source_path: path.to_path_buf(),
            backup_path: backup_path.clone(),
            written_at: now.format(LOG_TIME_FORMAT).map_err(|err| {
                AikitError::TargetWrite(format!("failed to format log time: {err}"))
            })?,
            status: "ok".to_string(),
        },
    )?;
    enforce_retention(&backup_dir, DEFAULT_RETENTION_PER_TARGET)?;
    Ok(Some(backup_path))
}

fn default_aikit_dir() -> Result<PathBuf> {
    let dirs = BaseDirs::new()
        .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
    Ok(dirs.home_dir().join(".aikit"))
}

fn unique_backup_path(backup_dir: &Path, timestamp: &str, source_path: &Path) -> PathBuf {
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_segment)
        .unwrap_or_else(|| "config".to_string());
    let mut candidate = backup_dir.join(format!("{timestamp}-{file_name}"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = backup_dir.join(format!("{timestamp}-{counter}-{file_name}"));
        counter += 1;
    }
    candidate
}

fn append_backup_log(aikit_dir: &Path, record: BackupLogRecord) -> Result<()> {
    let log_dir = aikit_dir.join("logs");
    fs::create_dir_all(&log_dir)?;
    let line = serde_json::to_string(&record)
        .map_err(|err| AikitError::TargetWrite(format!("failed to serialize backup log: {err}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("backups.jsonl"))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn enforce_retention(backup_dir: &Path, keep: usize) -> Result<()> {
    let mut files = fs::read_dir(backup_dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_file())
                .map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    files.sort();
    let remove_count = files.len().saturating_sub(keep);
    for path in files.into_iter().take(remove_count) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
