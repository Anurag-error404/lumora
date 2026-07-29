//! Recent search history (FTS + filter syntax), recorded automatically when
//! the user runs a search. Oldest entries beyond [`MAX_RECENT`] are pruned.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::SavedSearch;

const MAX_RECENT: usize = 30;

pub fn list(conn: &Connection) -> AppResult<Vec<SavedSearch>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, query, created_at, updated_at
         FROM saved_searches
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SavedSearch {
            id: r.get(0)?,
            name: r.get(1)?,
            query: r.get(2)?,
            created_at: r.get(3)?,
            updated_at: r.get(4)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Upsert a query into recent history and bump it to the top.
pub fn record(conn: &Connection, query: &str) -> AppResult<SavedSearch> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::msg("search query required"));
    }
    let now = Utc::now().to_rfc3339();
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM saved_searches WHERE lower(query) = lower(?1)",
            params![query],
            |r| r.get(0),
        )
        .optional()?;

    let id = if let Some(id) = existing_id {
        conn.execute(
            "UPDATE saved_searches
             SET name = ?1, query = ?1, updated_at = ?2
             WHERE id = ?3",
            params![query, now, id],
        )?;
        id
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO saved_searches (id, name, query, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, query, query, now, now],
        )?;
        id
    };

    prune(conn)?;

    conn.query_row(
        "SELECT id, name, query, created_at, updated_at FROM saved_searches WHERE id = ?1",
        params![id],
        |r| {
            Ok(SavedSearch {
                id: r.get(0)?,
                name: r.get(1)?,
                query: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .map_err(AppError::from)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let changed = conn.execute("DELETE FROM saved_searches WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(AppError::msg("recent search not found"));
    }
    Ok(())
}

pub fn clear(conn: &Connection) -> AppResult<usize> {
    let n = conn.execute("DELETE FROM saved_searches", [])?;
    Ok(n)
}

fn prune(conn: &Connection) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT id FROM saved_searches ORDER BY updated_at DESC")?;
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    if ids.len() <= MAX_RECENT {
        return Ok(());
    }
    for id in ids.into_iter().skip(MAX_RECENT) {
        conn.execute("DELETE FROM saved_searches WHERE id = ?1", params![id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn record_upserts_and_orders_by_recency() {
        let (_dir, conn) = open();
        record(&conn, "beach").unwrap();
        record(&conn, "dog").unwrap();
        record(&conn, "BEACH").unwrap(); // same query, case-insensitive

        let listed = list(&conn).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].query, "BEACH");
        assert_eq!(listed[1].query, "dog");
    }

    #[test]
    fn rejects_blank_query() {
        let (_dir, conn) = open();
        assert!(record(&conn, "  ").is_err());
    }

    #[test]
    fn prune_keeps_only_max_recent() {
        let (_dir, conn) = open();
        for i in 0..(MAX_RECENT + 5) {
            record(&conn, &format!("query-{i}")).unwrap();
        }
        assert_eq!(list(&conn).unwrap().len(), MAX_RECENT);
        assert_eq!(
            list(&conn).unwrap()[0].query,
            format!("query-{}", MAX_RECENT + 4)
        );
    }

    #[test]
    fn clear_removes_all() {
        let (_dir, conn) = open();
        record(&conn, "a").unwrap();
        record(&conn, "b").unwrap();
        assert_eq!(clear(&conn).unwrap(), 2);
        assert!(list(&conn).unwrap().is_empty());
    }
}
