// src/lib.rs
// This file is optional, but useful for exposing modules when used as a library

pub mod domain;

pub mod cli;
pub mod command;
pub mod config;
pub mod filesystem;
pub mod graph;
pub mod installation;
pub mod package;
pub mod package_installer;
pub mod package_list_command;
pub mod package_repo;
pub mod package_validate_command;
pub mod package_validator;
pub mod progress;
pub mod progress_display;
