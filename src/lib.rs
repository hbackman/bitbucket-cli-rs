//! `bb` — Bitbucket Cloud command-line tool.
//!
//! Modules are re-exported here so `tests/` integration tests and `main.rs`
//! share a single canonical definition.

pub mod api;
pub mod auth;
pub mod bbrepo;
pub mod cli;
pub mod config;
pub mod context;
pub mod error;
pub mod git;
pub mod iostreams;
pub mod text;
pub mod update;
