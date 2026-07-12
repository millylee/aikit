pub mod backup;
pub mod claude;
pub mod codex;
pub mod detect;
pub mod gemini;

use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSelection {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetWriteResult {
    pub target_id: String,
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
}

pub trait TargetWriter {
    fn target_id(&self) -> &'static str;
    fn default_path(&self) -> Result<PathBuf>;
    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult>;
}
