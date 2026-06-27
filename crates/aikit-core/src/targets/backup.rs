use std::{
    fs,
    path::{Path, PathBuf},
};

use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};

use crate::Result;

const BACKUP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year][month][day]-[hour][minute][second].[subsecond digits:3]");

pub fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let timestamp = OffsetDateTime::now_utc().format(BACKUP_FORMAT).unwrap();
    let backup_path = path.with_extension(format!("bak.{timestamp}"));
    fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}
