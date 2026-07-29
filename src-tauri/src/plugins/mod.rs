//! Lumora plugin subsystem (Phase 3 / Milestone 1 - Registry).
//!
//! # Module layout
//!
//! - `mod.rs` - public re-exports and top-level scan helper
//! - `manifest.rs` - parse + validate lumora.plugin.json
//! - `registry.rs` - scan dir, install, remove
//! - `permissions.rs` - permission token checks
//! - `history.rs` - read/write/trim history.jsonl per plugin
//! - `host.rs` - JS runtime + lumora.* bindings

pub mod analyze;
pub mod editor;
pub mod history;
pub mod host;
pub mod manifest;
pub mod permissions;
pub mod registry;
pub mod scaffold;

pub use analyze::{analyze_main_js, PluginAnalysis};
pub use editor::{
    fork_plugin, read_sources, read_sources_from_dir, save_draft, ForkPluginSpec, PluginSources,
    SavePluginDraft, SavePluginResult,
};
pub use history::{clear_all_history, clear_history, append_record, read_records, PluginRunRecord};
pub use manifest::PluginManifest;
pub use registry::{install_plugin_dir, plugin_dir, remove_plugin_dir, resolve_examples_dir, scan, PluginEntry};
pub use scaffold::{create_plugin, CreatePluginResult, CreatePluginSpec};
