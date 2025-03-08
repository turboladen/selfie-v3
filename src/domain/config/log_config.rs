use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use validator::Validate;

use super::{LOG_MAX_FILES_DEFAULT, LOG_MAX_SIZE_DEFAULT};

#[derive(Debug, Clone, PartialEq, Deserialize, Validate)]
#[validate(context = Self)]
pub struct LogConfig {
    #[serde(default)]
    pub enabled: bool,

    #[validate(custom(function = "validate_log_directory", use_context))]
    pub directory: PathBuf,

    #[serde(default = "default_max_files")]
    pub max_files: NonZeroUsize,

    #[serde(default = "default_max_size")]
    pub max_size: NonZeroUsize,
}

const fn default_max_files() -> NonZeroUsize {
    LOG_MAX_FILES_DEFAULT
}

const fn default_max_size() -> NonZeroUsize {
    LOG_MAX_SIZE_DEFAULT
}

/// Custom validator for log directory when logging is enabled
fn validate_log_directory(
    log_dir: &Path,
    config: &LogConfig,
) -> Result<(), validator::ValidationError> {
    if config.enabled {
        super::validate_path(log_dir)
    } else {
        Ok(())
    }
}
