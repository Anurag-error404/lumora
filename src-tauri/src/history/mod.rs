use std::collections::HashSet;

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{ActivityEntry, ExportRecord};
use crate::trash;

/// Reversible action applied by undo / redo.
#[derive(Debug, Clone)]
pub enum HistoryAction {
    SoftDelete {
        asset_ids: Vec<String>,
    },
    Restore {
        asset_ids: Vec<String>,
    },
    AddToAlbum {
        album_id: String,
        asset_ids: Vec<String>,
    },
    RemoveFromAlbum {
        album_id: String,
        asset_ids: Vec<String>,
    },
    SetFavorites {
        asset_ids: Vec<String>,
        favorite: bool,
    },
    /// Per-asset ratings; `ratings` is parallel to `asset_ids`.
    SetRatings {
        asset_ids: Vec<String>,
        ratings: Vec<i64>,
    },
    /// Per-asset colour labels; `labels` is parallel to `asset_ids`.
    SetColorLabels {
        asset_ids: Vec<String>,
        labels: Vec<Option<String>>,
    },
}

impl HistoryAction {
    pub fn asset_ids(&self) -> &[String] {
        match self {
            HistoryAction::SoftDelete { asset_ids }
            | HistoryAction::Restore { asset_ids }
            | HistoryAction::AddToAlbum { asset_ids, .. }
            | HistoryAction::RemoveFromAlbum { asset_ids, .. }
            | HistoryAction::SetFavorites { asset_ids, .. }
            | HistoryAction::SetRatings { asset_ids, .. }
            | HistoryAction::SetColorLabels { asset_ids, .. } => asset_ids,
        }
    }

    pub fn album_id(&self) -> Option<&str> {
        match self {
            HistoryAction::AddToAlbum { album_id, .. }
            | HistoryAction::RemoveFromAlbum { album_id, .. } => Some(album_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub created_at: String,
    pub undo: HistoryAction,
    pub redo: HistoryAction,
}

impl HistoryEntry {
    pub fn references_any_asset(&self, ids: &HashSet<String>) -> bool {
        self.undo.asset_ids().iter().any(|id| ids.contains(id))
            || self.redo.asset_ids().iter().any(|id| ids.contains(id))
    }
}

#[derive(Default)]
pub struct HistoryStacks {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
}

impl HistoryStacks {
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Drop undo/redo entries that reference any of the given asset IDs.
    /// Used after permanent delete / empty trash so those ops cannot be retried.
    pub fn invalidate_assets(&mut self, asset_ids: &[String]) -> usize {
        if asset_ids.is_empty() {
            return 0;
        }
        let set: HashSet<String> = asset_ids.iter().cloned().collect();
        let before = self.undo.len() + self.redo.len();
        self.undo.retain(|e| !e.references_any_asset(&set));
        self.redo.retain(|e| !e.references_any_asset(&set));
        before - (self.undo.len() + self.redo.len())
    }

    /// Drop undo/redo entries that target a deleted album.
    pub fn invalidate_album(&mut self, album_id: &str) -> usize {
        let before = self.undo.len() + self.redo.len();
        self.undo
            .retain(|e| e.undo.album_id() != Some(album_id) && e.redo.album_id() != Some(album_id));
        self.redo
            .retain(|e| e.undo.album_id() != Some(album_id) && e.redo.album_id() != Some(album_id));
        before - (self.undo.len() + self.redo.len())
    }

    /// Drop entries whose next apply direction is no longer possible in the DB.
    pub fn prune_invalid(&mut self, conn: &Connection) -> usize {
        let before = self.undo.len() + self.redo.len();
        self.undo.retain(|e| action_is_applicable(conn, &e.undo));
        self.redo.retain(|e| action_is_applicable(conn, &e.redo));
        before - (self.undo.len() + self.redo.len())
    }

    /// Pop the next undo entry that is still applicable, discarding stale ones.
    pub fn pop_valid_undo(&mut self, conn: &Connection) -> Option<HistoryEntry> {
        while let Some(entry) = self.undo.pop() {
            if action_is_applicable(conn, &entry.undo) {
                return Some(entry);
            }
        }
        None
    }

    /// Pop the next redo entry that is still applicable, discarding stale ones.
    pub fn pop_valid_redo(&mut self, conn: &Connection) -> Option<HistoryEntry> {
        while let Some(entry) = self.redo.pop() {
            if action_is_applicable(conn, &entry.redo) {
                return Some(entry);
            }
        }
        None
    }

    pub fn list_undo(&self) -> Vec<ActivityEntry> {
        self.undo
            .iter()
            .rev()
            .map(|e| ActivityEntry {
                id: e.id.clone(),
                kind: e.kind.clone(),
                label: e.label.clone(),
                detail: Some("Undoable".into()),
                created_at: e.created_at.clone(),
                undone: false,
            })
            .collect()
    }

    pub fn list_redo(&self) -> Vec<ActivityEntry> {
        self.redo
            .iter()
            .rev()
            .map(|e| ActivityEntry {
                id: e.id.clone(),
                kind: e.kind.clone(),
                label: e.label.clone(),
                detail: Some("Redoable".into()),
                created_at: e.created_at.clone(),
                undone: true,
            })
            .collect()
    }
}

pub fn make_entry(
    kind: &str,
    label: impl Into<String>,
    undo: HistoryAction,
    redo: HistoryAction,
) -> HistoryEntry {
    HistoryEntry {
        id: Uuid::new_v4().to_string(),
        label: label.into(),
        kind: kind.to_string(),
        created_at: Utc::now().to_rfc3339(),
        undo,
        redo,
    }
}

/// True when applying `action` would still do something meaningful against current DB state.
pub fn action_is_applicable(conn: &Connection, action: &HistoryAction) -> bool {
    match action {
        HistoryAction::SoftDelete { asset_ids } => {
            !asset_ids.is_empty() && asset_ids.iter().all(|id| asset_exists_live(conn, id))
        }
        HistoryAction::Restore { asset_ids } => {
            !asset_ids.is_empty() && asset_ids.iter().all(|id| asset_exists_trashed(conn, id))
        }
        HistoryAction::SetFavorites { asset_ids, .. }
        | HistoryAction::SetRatings { asset_ids, .. }
        | HistoryAction::SetColorLabels { asset_ids, .. } => {
            !asset_ids.is_empty() && asset_ids.iter().all(|id| asset_exists(conn, id))
        }
        HistoryAction::AddToAlbum {
            album_id,
            asset_ids,
        }
        | HistoryAction::RemoveFromAlbum {
            album_id,
            asset_ids,
        } => {
            album_exists(conn, album_id)
                && !asset_ids.is_empty()
                && asset_ids.iter().all(|id| asset_exists(conn, id))
        }
    }
}

fn asset_exists(conn: &Connection, id: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM assets WHERE id = ?1",
        params![id],
        |_| Ok(()),
    )
    .is_ok()
}

fn asset_exists_live(conn: &Connection, id: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM assets WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |_| Ok(()),
    )
    .is_ok()
}

fn asset_exists_trashed(conn: &Connection, id: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM assets WHERE id = ?1 AND deleted_at IS NOT NULL",
        params![id],
        |_| Ok(()),
    )
    .is_ok()
}

fn album_exists(conn: &Connection, id: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM albums WHERE id = ?1",
        params![id],
        |_| Ok(()),
    )
    .is_ok()
}

pub fn apply_action(conn: &Connection, action: &HistoryAction) -> AppResult<()> {
    if !action_is_applicable(conn, action) {
        return Err(crate::error::AppError::msg(
            "that action is no longer valid (items were permanently deleted or changed)",
        ));
    }
    match action {
        HistoryAction::SoftDelete { asset_ids } => {
            trash::soft_delete(conn, asset_ids)?;
        }
        HistoryAction::Restore { asset_ids } => {
            trash::restore(conn, asset_ids)?;
        }
        HistoryAction::AddToAlbum {
            album_id,
            asset_ids,
        } => {
            for asset_id in asset_ids {
                conn.execute(
                    "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
                    params![album_id, asset_id],
                )?;
            }
        }
        HistoryAction::RemoveFromAlbum {
            album_id,
            asset_ids,
        } => {
            for asset_id in asset_ids {
                conn.execute(
                    "DELETE FROM album_assets WHERE album_id=?1 AND asset_id=?2",
                    params![album_id, asset_id],
                )?;
            }
        }
        HistoryAction::SetFavorites {
            asset_ids,
            favorite,
        } => {
            let fav = if *favorite { 1 } else { 0 };
            for asset_id in asset_ids {
                conn.execute(
                    "UPDATE assets SET favorite=?1 WHERE id=?2",
                    params![fav, asset_id],
                )?;
            }
        }
        HistoryAction::SetRatings { asset_ids, ratings } => {
            for (asset_id, rating) in asset_ids.iter().zip(ratings.iter()) {
                conn.execute(
                    "UPDATE assets SET rating=?1 WHERE id=?2",
                    params![rating, asset_id],
                )?;
            }
        }
        HistoryAction::SetColorLabels { asset_ids, labels } => {
            for (asset_id, label) in asset_ids.iter().zip(labels.iter()) {
                conn.execute(
                    "UPDATE assets SET color_label=?1 WHERE id=?2",
                    params![label, asset_id],
                )?;
            }
        }
    }
    Ok(())
}

pub fn record_activity(
    conn: &Connection,
    kind: &str,
    label: &str,
    detail: Option<&str>,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO activity_log (id, kind, label, detail, created_at, undone)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![id, kind, label, detail, created_at],
    )?;
    Ok(id)
}

pub fn list_activity(conn: &Connection, limit: u32) -> AppResult<Vec<ActivityEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, label, detail, created_at, undone
         FROM activity_log
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ActivityEntry {
            id: row.get(0)?,
            kind: row.get(1)?,
            label: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
            undone: row.get::<_, i64>(5)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Persist a local-only import performance sample (no telemetry).
pub fn record_import_run(
    conn: &Connection,
    result: &crate::models::ImportResult,
    roots: &[std::path::PathBuf],
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let finished_at = Utc::now();
    let started_at = finished_at - chrono::Duration::milliseconds(result.duration_ms as i64);
    let roots_json = serde_json::to_string(
        &roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let note = if result.cancelled {
        Some("cancelled")
    } else {
        None
    };
    conn.execute(
        "INSERT INTO import_runs (
            id, started_at, finished_at, duration_ms, scanned, inserted, updated,
            skipped, cancelled, files_per_sec, roots_json, note
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            id,
            started_at.to_rfc3339(),
            finished_at.to_rfc3339(),
            result.duration_ms as i64,
            result.scanned as i64,
            result.inserted as i64,
            result.updated as i64,
            result.skipped as i64,
            if result.cancelled { 1i64 } else { 0 },
            result.files_per_sec,
            roots_json,
            note,
        ],
    )?;

    let label = if result.cancelled {
        format!(
            "Import stopped — {} files in {:.1}s ({:.1}/s)",
            result.scanned,
            result.duration_ms as f64 / 1000.0,
            result.files_per_sec
        )
    } else {
        format!(
            "Imported {} files in {:.1}s ({:.1}/s)",
            result.scanned,
            result.duration_ms as f64 / 1000.0,
            result.files_per_sec
        )
    };
    let detail = format!(
        "inserted={}, updated={}, skipped={}, duration_ms={}, files_per_sec={:.2}",
        result.inserted, result.updated, result.skipped, result.duration_ms, result.files_per_sec
    );
    let _ = record_activity(conn, "import", &label, Some(&detail));
    Ok(id)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRun {
    pub id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub scanned: i64,
    pub inserted: i64,
    pub updated: i64,
    pub skipped: i64,
    pub cancelled: bool,
    pub files_per_sec: Option<f64>,
    pub roots_json: Option<String>,
    pub note: Option<String>,
}

pub fn list_import_runs(conn: &Connection, limit: u32) -> AppResult<Vec<ImportRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, started_at, finished_at, duration_ms, scanned, inserted, updated,
                skipped, cancelled, files_per_sec, roots_json, note
         FROM import_runs
         ORDER BY finished_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ImportRun {
            id: row.get(0)?,
            started_at: row.get(1)?,
            finished_at: row.get(2)?,
            duration_ms: row.get(3)?,
            scanned: row.get(4)?,
            inserted: row.get(5)?,
            updated: row.get(6)?,
            skipped: row.get(7)?,
            cancelled: row.get::<_, i64>(8)? != 0,
            files_per_sec: row.get(9)?,
            roots_json: row.get(10)?,
            note: row.get(11)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn record_export(
    conn: &Connection,
    path: &str,
    asset_count: u32,
    exported: u32,
    missing: u32,
    note: Option<&str>,
) -> AppResult<ExportRecord> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO exports (id, path, asset_count, exported_count, missing_count, created_at, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, path, asset_count, exported, missing, created_at, note],
    )?;
    Ok(ExportRecord {
        id,
        path: path.to_string(),
        asset_count: asset_count as i64,
        exported_count: exported as i64,
        missing_count: missing as i64,
        created_at,
        note: note.map(|s| s.to_string()),
    })
}

pub fn list_exports(conn: &Connection, limit: u32) -> AppResult<Vec<ExportRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, asset_count, exported_count, missing_count, created_at, note
         FROM exports
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ExportRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            asset_count: row.get(2)?,
            exported_count: row.get(3)?,
            missing_count: row.get(4)?,
            created_at: row.get(5)?,
            note: row.get(6)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a1', '/tmp/a1.jpg', 'h1', 'image', '2020-01-01', '2020-01-01'),
                    ('a2', '/tmp/a2.jpg', 'h2', 'image', '2020-01-01', '2020-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, name, created_at) VALUES ('alb1', 'Trip', '2020-01-01')",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    #[test]
    fn record_and_list_exports() {
        let (_dir, conn) = setup();
        record_export(&conn, "/tmp/out.zip", 2, 2, 0, Some("test")).unwrap();
        let list = list_exports(&conn, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "/tmp/out.zip");
        assert_eq!(list[0].exported_count, 2);
    }

    #[test]
    fn undo_redo_album_add() {
        let (_dir, conn) = setup();
        let mut stacks = HistoryStacks::default();
        let entry = make_entry(
            "album",
            "Added 2 to Trip",
            HistoryAction::RemoveFromAlbum {
                album_id: "alb1".into(),
                asset_ids: vec!["a1".into(), "a2".into()],
            },
            HistoryAction::AddToAlbum {
                album_id: "alb1".into(),
                asset_ids: vec!["a1".into(), "a2".into()],
            },
        );
        apply_action(
            &conn,
            &HistoryAction::AddToAlbum {
                album_id: "alb1".into(),
                asset_ids: vec!["a1".into(), "a2".into()],
            },
        )
        .unwrap();
        stacks.push(entry);

        let undone = stacks.undo.pop().unwrap();
        apply_action(&conn, &undone.undo).unwrap();
        stacks.redo.push(undone);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM album_assets WHERE album_id='alb1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        let redone = stacks.redo.pop().unwrap();
        apply_action(&conn, &redone.redo).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM album_assets WHERE album_id='alb1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn permanent_delete_invalidates_undo_and_redo() {
        let (_dir, conn) = setup();
        let mut stacks = HistoryStacks::default();

        trash::soft_delete(&conn, &["a1".into()]).unwrap();
        stacks.push(make_entry(
            "trash",
            "Moved 1 to trash",
            HistoryAction::Restore {
                asset_ids: vec!["a1".into()],
            },
            HistoryAction::SoftDelete {
                asset_ids: vec!["a1".into()],
            },
        ));

        // Undo soft-delete → a1 is live again, then soft-delete again for redo path.
        let undone = stacks.undo.pop().unwrap();
        apply_action(&conn, &undone.undo).unwrap();
        stacks.redo.push(undone);
        assert!(stacks.can_redo());

        trash::soft_delete(&conn, &["a1".into()]).unwrap();
        trash::permanently_delete(&conn, &["a1".into()], false).unwrap();

        let dropped = stacks.invalidate_assets(&["a1".into()]);
        assert!(dropped >= 1);
        assert!(!stacks.can_undo());
        assert!(!stacks.can_redo());
        assert!(!action_is_applicable(
            &conn,
            &HistoryAction::Restore {
                asset_ids: vec!["a1".into()],
            }
        ));
    }

    #[test]
    fn prune_skips_stale_restore_after_permanent_delete() {
        let (_dir, conn) = setup();
        let mut stacks = HistoryStacks::default();
        trash::soft_delete(&conn, &["a1".into(), "a2".into()]).unwrap();
        stacks.push(make_entry(
            "trash",
            "Moved 2 to trash",
            HistoryAction::Restore {
                asset_ids: vec!["a1".into(), "a2".into()],
            },
            HistoryAction::SoftDelete {
                asset_ids: vec!["a1".into(), "a2".into()],
            },
        ));
        // Keep a2's undo valid by only permanently deleting a1 — whole entry becomes invalid
        // because Restore requires ALL assets still trashed.
        trash::permanently_delete(&conn, &["a1".into()], false).unwrap();
        let pruned = stacks.prune_invalid(&conn);
        assert_eq!(pruned, 1);
        assert!(stacks.pop_valid_undo(&conn).is_none());
    }
}
