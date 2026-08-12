//! Persisted memory cards.
//!
//! Grouping memories scans the whole library, so it never runs on the UI path.
//! A background builder calls [`rebuild`] and stores finished cards in
//! `memory_cards`; the Memories page reads them back with [`list`], which is a
//! single indexed select plus one cached-prose lookup per card.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Local, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::AppResult;

use super::{parse_memory_id, with_cached_prose, MemorySummary};

/// Cards kept on disk. `list_memories` clamps requests to 50.
const CACHE_SIZE: u32 = 50;

static BUILDING: AtomicBool = AtomicBool::new(false);
/// Starts dirty so the first builder tick after launch regroups the cards.
static DIRTY: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoriesStatus {
    /// A rebuild is running or queued — the UI shows a loader.
    pub building: bool,
    /// When the cards were last built. `None` means never.
    pub built_at: Option<String>,
    /// Cards currently readable (dismissed ones excluded).
    pub count: i64,
}

/// Ask the background builder to regroup memories on its next tick.
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// Claim a queued rebuild. Returns true at most once per [`mark_dirty`].
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::Relaxed)
}

/// True when the cards were last built on an earlier calendar day, which
/// retires the "on this day" card.
pub fn built_before_today(conn: &Connection) -> bool {
    match built_on(conn) {
        Ok(Some(day)) => day != Local::now().date_naive().to_string(),
        // Never built: the initial dirty flag already covers it.
        Ok(None) => false,
        Err(_) => false,
    }
}

pub fn status(conn: &Connection) -> AppResult<MemoriesStatus> {
    let built_at: Option<String> = conn
        .query_row(
            "SELECT built_at FROM memory_cache_state WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_cards
         WHERE id NOT IN (SELECT memory_id FROM dismissed_memories)",
        [],
        |r| r.get(0),
    )?;
    Ok(MemoriesStatus {
        building: BUILDING.load(Ordering::Relaxed) || DIRTY.load(Ordering::Relaxed),
        built_at,
        count,
    })
}

/// Read cached cards, newest grouping first. Dismissed ids are omitted.
pub fn list(conn: &Connection, limit: u32) -> AppResult<Vec<MemorySummary>> {
    let limit = limit.clamp(1, CACHE_SIZE);
    let mut stmt = conn.prepare(
        "SELECT id, title, subtitle, quote, asset_count, cover_asset_id, cover_thumbnail_path,
                start_date, end_date, place_label, person_name
         FROM memory_cards
         WHERE id NOT IN (SELECT memory_id FROM dismissed_memories)
         ORDER BY position
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        let id: String = r.get(0)?;
        Ok(MemorySummary {
            kind: parse_memory_id(&id)
                .map(|parsed| parsed.kind())
                .unwrap_or(super::MemoryKind::OnThisDay),
            id,
            title: r.get(1)?,
            subtitle: r.get(2)?,
            insight: String::new(),
            quote: r.get(3)?,
            prose: None,
            asset_count: r.get(4)?,
            cover_asset_id: r.get(5)?,
            cover_thumbnail_path: r.get(6)?,
            start_date: r.get(7)?,
            end_date: r.get(8)?,
            place_label: r.get(9)?,
            person_name: r.get(10)?,
        })
    })?;
    let cards: Vec<MemorySummary> = rows.filter_map(|r| r.ok()).collect();
    Ok(cards
        .into_iter()
        .map(|card| with_cached_prose(conn, card))
        .collect())
}

/// Regroup every memory and replace the cached cards. Returns the card count.
///
/// Concurrent calls are collapsed: the second one returns immediately, because
/// both would compute the same thing from the same library.
pub fn rebuild(conn: &Connection) -> AppResult<usize> {
    if BUILDING.swap(true, Ordering::SeqCst) {
        return Ok(0);
    }
    let result = replace_cards(conn);
    BUILDING.store(false, Ordering::SeqCst);
    result
}

fn replace_cards(conn: &Connection) -> AppResult<usize> {
    let cards = super::list_memories(conn, CACHE_SIZE)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM memory_cards", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO memory_cards
                (id, position, title, subtitle, quote, asset_count, cover_asset_id,
                 cover_thumbnail_path, start_date, end_date, place_label, person_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for (position, card) in cards.iter().enumerate() {
            stmt.execute(params![
                card.id,
                position as i64,
                card.title,
                card.subtitle,
                card.quote,
                card.asset_count,
                card.cover_asset_id,
                card.cover_thumbnail_path,
                card.start_date,
                card.end_date,
                card.place_label,
                card.person_name,
            ])?;
        }
    }
    tx.execute(
        "INSERT INTO memory_cache_state (id, built_at, built_on) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET built_at = excluded.built_at, built_on = excluded.built_on",
        params![
            Utc::now().to_rfc3339(),
            Local::now().date_naive().to_string()
        ],
    )?;
    tx.commit()?;
    Ok(cards.len())
}

fn built_on(conn: &Connection) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT built_on FROM memory_cache_state WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}