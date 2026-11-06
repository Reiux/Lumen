//! Lumen Core library
//!
//! Provides the core protocol, authentication, deduplication, album management,
//! file scanning, live photo pairing, rate limiting, and upload engine.

pub mod album;
pub mod api;
pub mod auth;
pub mod config;
pub mod dedup;
pub mod keyring;
pub mod livephoto;
pub mod net;
pub mod proto;
pub mod safety;
pub mod scanner;
pub mod updater;
pub mod upload;
pub mod watcher;
