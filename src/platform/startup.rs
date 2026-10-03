#[cfg(target_os = "windows")]
#[path = "windows/startup.rs"]
mod backend;
#[cfg(target_os = "linux")]
pub use super::linux::{configure, migrate_legacy};
#[cfg(target_os = "windows")]
pub use backend::{configure, migrate_legacy};
