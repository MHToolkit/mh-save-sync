#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("unsupported or invalid save: {0}")]
    InvalidSave(String),
    #[error("unsafe install refused: {0}")]
    UnsafeInstall(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
