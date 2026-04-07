//! Shared host ABI assets for Ignis.
//!
//! This crate currently exposes the canonical WIT contract used by guest SDKs and host runtimes.

pub const SQLITE_WORLD_WIT: &str = include_str!("../wit/world.wit");
