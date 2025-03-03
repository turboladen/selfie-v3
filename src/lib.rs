// src/lib.rs
// This file is optional, but useful for exposing modules when used as a library

pub mod adapters;
pub mod domain;
pub mod ports;
pub mod services;

pub mod cli;
pub mod package_installer;
pub mod package_list_command;
pub mod package_validate_command;
pub mod package_validator;
pub mod progress;
pub mod progress_display;
