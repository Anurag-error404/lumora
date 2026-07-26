use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::AppResult;

#[derive(Debug, Default)]
pub struct DirectoryStats {
    pub file_count: u64,
    pub size_bytes: u64,
}

pub fn directory_stats(path: &Path) -> DirectoryStats {
    let mut stats = DirectoryStats::default();
    let Ok(entries) = fs::read_dir(path) else {
        return stats;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() {
            stats.file_count += 1;
            stats.size_bytes += metadata.len();
        }
    }
    stats
}

pub fn latest_log_lines(logs_dir: &Path, max_lines: usize) -> AppResult<Vec<String>> {
    let Some(path) = newest_file(logs_dir) else {
        return Ok(Vec::new());
    };
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..]
        .iter()
        .map(|line| (*line).to_string())
        .collect())
}

pub fn error_log_lines(lines: &[String], max_lines: usize) -> Vec<String> {
    let mut errors: Vec<String> = lines
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("panic")
                || lower.contains("panick")
                || lower.contains("fatal")
                || lower.contains("crash")
        })
        .cloned()
        .collect();
    if errors.len() > max_lines {
        errors.drain(0..errors.len() - max_lines);
    }
    errors
}

fn newest_file(path: &Path) -> Option<PathBuf> {
    fs::read_dir(path)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tail_and_filters_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photovault.log.2026-07-26");
        fs::write(
            &path,
            "INFO started\nWARN recovered\nERROR thumbnail failed\nthread panicked here\n",
        )
        .unwrap();

        let lines = latest_log_lines(dir.path(), 3).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "WARN recovered");
        let errors = error_log_lines(&lines, 10);
        assert_eq!(errors.len(), 2);

        let stats = directory_stats(dir.path());
        assert_eq!(stats.file_count, 1);
        assert!(stats.size_bytes > 0);
    }
}
