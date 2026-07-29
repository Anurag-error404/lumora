//! Plugin execution history: read, write, trim, and clear `history.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const HISTORY_FILE: &str = "history.jsonl";
/// Maximum run records retained per plugin.
pub const MAX_HISTORY_RECORDS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunRecord {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub action_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub mode: String,
    pub outcome: RunOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub assets_requested: u32,
    pub assets_affected: u32,
    pub assets_skipped: u32,
    pub log_lines: Vec<PluginLogLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunOutcome {
    Ok,
    Cancelled,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLogLine {
    pub level: LogLevel,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl PluginRunRecord {
    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}

/// Append a run record to the plugin's `history.jsonl` and trim to `MAX_HISTORY_RECORDS`.
pub fn append_record(plugin_dir: &Path, record: &PluginRunRecord) -> AppResult<()> {
    let path = plugin_dir.join(HISTORY_FILE);
    let line = serde_json::to_string(record)
        .map_err(|e| AppError::msg(format!("serialize run record: {e}")))?;

    // Read existing records, append new one, trim, rewrite atomically.
    let mut records: Vec<String> = if path.exists() {
        let file = fs::File::open(&path)?;
        BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .filter(|l| !l.trim().is_empty())
            .collect()
    } else {
        Vec::new()
    };

    records.push(line);

    // Keep only the last MAX_HISTORY_RECORDS entries.
    if records.len() > MAX_HISTORY_RECORDS {
        let drop = records.len() - MAX_HISTORY_RECORDS;
        records.drain(..drop);
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    for rec in &records {
        writeln!(file, "{rec}")?;
    }
    Ok(())
}

/// Read the last `limit` records for a plugin, newest first.
pub fn read_records(plugin_dir: &Path, limit: usize) -> AppResult<Vec<PluginRunRecord>> {
    let path = plugin_dir.join(HISTORY_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path)?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.trim().is_empty())
        .collect();

    let records: Vec<PluginRunRecord> = lines
        .iter()
        .rev()
        .take(limit)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    Ok(records)
}

/// Delete `history.jsonl` for a plugin.
pub fn clear_history(plugin_dir: &Path) -> AppResult<()> {
    let path = plugin_dir.join(HISTORY_FILE);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Delete `history.jsonl` for every plugin under `plugins_dir`.
pub fn clear_all_history(plugins_dir: &Path) -> AppResult<()> {
    if !plugins_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(plugins_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let history = entry.path().join(HISTORY_FILE);
            if history.exists() {
                fs::remove_file(history)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_record(plugin_id: &str, action_id: &str) -> PluginRunRecord {
        PluginRunRecord {
            run_id: PluginRunRecord::new_id(),
            plugin_id: plugin_id.into(),
            plugin_version: "1.0.0".into(),
            action_id: action_id.into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:00:01Z".into(),
            duration_ms: 1000,
            mode: "apply".into(),
            outcome: RunOutcome::Ok,
            error_code: None,
            error_message: None,
            assets_requested: 5,
            assets_affected: 5,
            assets_skipped: 0,
            log_lines: Vec::new(),
        }
    }

    #[test]
    fn append_and_read() {
        let tmp = tempdir().unwrap();
        let rec = make_record("com.test", "action1");
        append_record(tmp.path(), &rec).unwrap();
        let records = read_records(tmp.path(), 20).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].plugin_id, "com.test");
    }

    #[test]
    fn trims_to_max() {
        let tmp = tempdir().unwrap();
        for _ in 0..(MAX_HISTORY_RECORDS + 10) {
            append_record(tmp.path(), &make_record("com.test", "a")).unwrap();
        }
        let records = read_records(tmp.path(), MAX_HISTORY_RECORDS + 100).unwrap();
        assert_eq!(records.len(), MAX_HISTORY_RECORDS);
    }

    #[test]
    fn clear_removes_file() {
        let tmp = tempdir().unwrap();
        append_record(tmp.path(), &make_record("com.test", "a")).unwrap();
        clear_history(tmp.path()).unwrap();
        assert!(!tmp.path().join(HISTORY_FILE).exists());
    }
}
