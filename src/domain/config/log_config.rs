use std::path::PathBuf;

use serde::Deserialize;

use super::{LOG_MAX_FILES_DEFAULT, LOG_MAX_SIZE_DEFAULT};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LogConfig {
    #[serde(default)]
    pub enabled: bool,

    pub directory: PathBuf,

    #[serde(default = "default_max_files")]
    pub max_files: usize,

    #[serde(default = "default_max_size")]
    pub max_size: usize,
}

const fn default_max_files() -> usize {
    LOG_MAX_FILES_DEFAULT
}

const fn default_max_size() -> usize {
    LOG_MAX_SIZE_DEFAULT
}
