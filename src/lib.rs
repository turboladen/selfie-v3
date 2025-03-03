// src/lib.rs
// This file is optional, but useful for exposing modules when used as a library

pub mod adapters;
pub mod domain;
pub mod ports;

pub mod cli;
pub mod filesystem;
pub mod installation_manager;
pub mod package_installer;
pub mod package_list_command;
pub mod package_repo;
pub mod package_validate_command;
pub mod package_validator;
pub mod progress;
pub mod progress_display;
