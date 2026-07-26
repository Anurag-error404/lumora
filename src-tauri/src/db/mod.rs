use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::AppResult;

const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_exports_activity.sql");
const MIGRATION_003: &str = include_str!("../../migrations/003_recently_viewed.sql");
const MIGRATION_004: &str = include_str!("../../migrations/004_fts_stored_content.sql");
const MIGRATION_005: &str = include_str!("../../migrations/005_vault.sql");
const MIGRATION_006: &str = include_str!("../../migrations/006_vault_groups.sql");
const MIGRATION_007: &str = include_str!("../../migrations/007_multi_vault.sql");
const MIGRATION_008: &str = include_str!("../../migrations/008_ml_phase2.sql");
const MIGRATION_009: &str = include_str!("../../migrations/009_saved_searches.sql");
const MIGRATION_010: &str = include_str!("../../migrations/010_ocr.sql");
const MIGRATION_011: &str = include_str!("../../migrations/011_faces.sql");
const MIGRATION_012: &str = include_str!("../../migrations/012_face_ignore.sql");

pub fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(MIGRATION_001)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (1)", [])?;
        tracing::info!("applied migration 001_init");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 2 {
        conn.execute_batch(MIGRATION_002)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (2)", [])?;
        tracing::info!("applied migration 002_exports_activity");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 3 {
        conn.execute_batch(MIGRATION_003)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (3)", [])?;
        tracing::info!("applied migration 003_recently_viewed");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 4 {
        conn.execute_batch(MIGRATION_004)?;
        // Repopulate FTS now that columns are stored (contentless broke JOINs).
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for id in ids {
            crate::indexer::refresh_fts(conn, &id)?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (4)", [])?;
        tracing::info!("applied migration 004_fts_stored_content");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 5 {
        conn.execute_batch(MIGRATION_005)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (5)", [])?;
        tracing::info!("applied migration 005_vault");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 6 {
        conn.execute_batch(MIGRATION_006)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (6)", [])?;
        tracing::info!("applied migration 006_vault_groups");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 7 {
        conn.execute_batch(MIGRATION_007)?;
        migrate_vault_config_to_vaults(conn)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (7)", [])?;
        tracing::info!("applied migration 007_multi_vault");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 8 {
        conn.execute_batch(MIGRATION_008)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (8)", [])?;
        tracing::info!("applied migration 008_ml_phase2");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 9 {
        conn.execute_batch(MIGRATION_009)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (9)", [])?;
        tracing::info!("applied migration 009_saved_searches");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 10 {
        conn.execute_batch(MIGRATION_010)?;
        // Repopulate FTS so the new ocr_text column is present for every asset.
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for id in ids {
            crate::indexer::refresh_fts(conn, &id)?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (10)", [])?;
        tracing::info!("applied migration 010_ocr");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 11 {
        conn.execute_batch(MIGRATION_011)?;
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for id in ids {
            crate::indexer::refresh_fts(conn, &id)?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (11)", [])?;
        tracing::info!("applied migration 011_faces");
    }

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 12 {
        conn.execute_batch(MIGRATION_012)?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (12)", [])?;
        tracing::info!("applied migration 012_face_ignore");
    }

    Ok(())
}

/// Copy the legacy single-row `vault_config` into `vaults`, backfill `vault_id`
/// on locked rows, then drop the old table.
fn migrate_vault_config_to_vaults(conn: &Connection) -> AppResult<()> {
    let legacy = conn
        .query_row(
            "SELECT vault_path, salt, wrap_nonce, wrapped_key, kdf_m_cost, kdf_t_cost, kdf_p_cost,
                    created_at, recovery_salt, recovery_nonce, recovery_wrapped_key
             FROM vault_config WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, u32>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, u32>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()?;

    let vault_id = if let Some((
        vault_path,
        salt,
        wrap_nonce,
        wrapped_key,
        m,
        t,
        p,
        created_at,
        rec_salt,
        rec_nonce,
        rec_wrapped,
    )) = legacy
    {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO vaults
               (id, name, vault_path, salt, wrap_nonce, wrapped_key, kdf_m_cost, kdf_t_cost,
                kdf_p_cost, created_at, recovery_salt, recovery_nonce, recovery_wrapped_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                "Locked folder",
                vault_path,
                salt,
                wrap_nonce,
                wrapped_key,
                m,
                t,
                p,
                created_at,
                rec_salt,
                rec_nonce,
                rec_wrapped,
            ],
        )?;
        Some(id)
    } else {
        None
    };

    if let Some(ref vault_id) = vault_id {
        conn.execute(
            "INSERT INTO locked_albums_v2 (id, vault_id, name_enc, created_at)
             SELECT id, ?1, name_enc, created_at FROM locked_albums",
            params![vault_id],
        )?;
        conn.execute(
            "INSERT INTO locked_assets_v3
               (id, vault_id, vault_file, thumb_file, meta_enc, locked_album_id, locked_at,
                legacy_file_name, legacy_media_type, legacy_width, legacy_height,
                legacy_size_bytes, legacy_original_path)
             SELECT id, ?1, vault_file, thumb_file, meta_enc, locked_album_id, locked_at,
                    legacy_file_name, legacy_media_type, legacy_width, legacy_height,
                    legacy_size_bytes, legacy_original_path
             FROM locked_assets",
            params![vault_id],
        )?;
    } else {
        // No vault configured — locked tables must be empty; still swap to the
        // new schema so subsequent inserts require a vault_id.
        let album_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM locked_albums", [], |r| r.get(0))?;
        let asset_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM locked_assets", [], |r| r.get(0))?;
        if album_count > 0 || asset_count > 0 {
            return Err(crate::error::AppError::msg(
                "locked items exist without a vault configuration",
            ));
        }
    }

    conn.execute_batch(
        "DROP TABLE locked_assets;
         DROP TABLE locked_albums;
         ALTER TABLE locked_albums_v2 RENAME TO locked_albums;
         ALTER TABLE locked_assets_v3 RENAME TO locked_assets;
         CREATE INDEX IF NOT EXISTS idx_locked_assets_locked_at ON locked_assets(locked_at);
         CREATE INDEX IF NOT EXISTS idx_locked_assets_album ON locked_assets(locked_album_id);
         CREATE INDEX IF NOT EXISTS idx_locked_assets_vault ON locked_assets(vault_id);
         CREATE INDEX IF NOT EXISTS idx_locked_albums_vault ON locked_albums(vault_id);
         DROP TABLE IF EXISTS vault_config;",
    )?;

    Ok(())
}

pub fn open_and_migrate(path: &std::path::Path) -> AppResult<Connection> {
    let conn = crate::state::open_db(path)?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn db_migrations_fresh_and_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = open_and_migrate(&path).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.iter().any(|t| t == "assets"));
        assert!(tables.iter().any(|t| t == "albums"));
        assert!(tables.iter().any(|t| t == "tags"));
        assert!(tables.iter().any(|t| t == "watched_folders"));
        // Faces/people shipped with migration 011.
        assert!(tables.iter().any(|t| t == "faces"));
        assert!(tables.iter().any(|t| t == "people"));

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 12);

        // Phase 2 derived-data tables.
        assert!(tables.iter().any(|t| t == "ml_models"));
        assert!(tables.iter().any(|t| t == "asset_embeddings"));
        assert!(tables.iter().any(|t| t == "ml_jobs"));
        assert!(tables.iter().any(|t| t == "asset_text"));
        assert!(tables.iter().any(|t| t == "exports"));
        assert!(tables.iter().any(|t| t == "activity_log"));
        assert!(tables.iter().any(|t| t == "asset_views"));
        assert!(tables.iter().any(|t| t == "vaults"));
        assert!(!tables.iter().any(|t| t == "vault_config"));
        assert!(tables.iter().any(|t| t == "locked_assets"));
        assert!(tables.iter().any(|t| t == "locked_albums"));
        assert!(tables.iter().any(|t| t == "saved_searches"));

        // reopen — no double apply
        drop(conn);
        let conn2 = open_and_migrate(&path).unwrap();
        let version2: i64 = conn2
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version2, 12);
    }
}
