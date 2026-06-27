use thiserror::Error;

pub type Result<T> = std::result::Result<T, AikitError>;

#[derive(Debug, Error)]
pub enum AikitError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse error: {0}")]
    ConfigParse(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("target write error: {0}")]
    TargetWrite(String),
}
