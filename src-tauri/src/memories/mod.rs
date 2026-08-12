//! Memories — local clustering + ranking + metadata templates (+ v1.5 CLIP/captions).
//!
//! No LLM, no network. A memory is a deterministic id + curated asset set +
//! title/subtitle filled from dates / people / places. Grouping is expensive,
//! so [`cache`] stores the finished cards and the UI reads only those.
//! v1.5 adds CLIP diversity ranking (when embeddings exist) and Florence captions
//! as quotes. Optional Save as album creates a normal user album; memories never
//! auto-create albums.

pub mod cache;
mod rank;
pub mod prose;

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{Album, AssetSummary};
use crate::search::map_asset;

use self::rank::{diversify, load_embeddings, pick_quote, RankCandidate, MAX_CANDIDATES};

const MIN_ON_THIS_DAY: i64 = 3;
const MIN_WEEKEND: i64 = 5;
const MIN_PERSON_PLACE: i64 = 5;
const MAX_WEEKEND_MEMORIES: usize = 12;
const MAX_PERSON_PLACE_MEMORIES: usize = 20;
const MAX_ASSETS_PER_MEMORY: u32 = 200;
/// A card shows one cover and one quote, and `pick_quote` scans 16 rows.
const SUMMARY_CANDIDATES: usize = 16;

const SELECT_COLUMNS: &str = "a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width,
     a.height, a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
     a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at";

const RANK_ORDER: &str = "(a.favorite * 10 + a.rating * 2 + CASE WHEN a.thumbnail_path IS NOT NULL THEN 1 ELSE 0 END) DESC,
         COALESCE(a.captured_at, a.created_at) DESC";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryKind {
    OnThisDay,
    WeekendTrip,
    PersonPlace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub id: String,
    pub kind: MemoryKind,
    pub title: String,
    /// Compact meta (dates / counts) — prefer [`insight`] for display copy.
    pub subtitle: String,
    /// Narrative line for UI: prose → caption insight → warm template.
    pub insight: String,
    /// Florence caption quote when available (v1.5); otherwise null.
    pub quote: Option<String>,
    /// Optional polished one-liner from the on-device prose model.
    pub prose: Option<String>,
    pub asset_count: i64,
    pub cover_asset_id: Option<String>,
    pub cover_thumbnail_path: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub place_label: Option<String>,
    pub person_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDetail {
    pub summary: MemorySummary,
    pub assets: Vec<AssetSummary>,
}

struct MetaRow {
    id: String,
    favorite: bool,
    rating: i64,
    thumbnail_path: Option<String>,
}

/// Load candidate metadata (base-ranked, capped), attach CLIP vectors, diversify.
fn diversified_ids(
    conn: &Connection,
    where_sql: &str,
    params: &[&dyn ToSql],
    take: usize,
) -> AppResult<Vec<String>> {
    let sql = format!(
        "SELECT a.id, a.favorite, a.rating, a.thumbnail_path
         FROM assets a
         WHERE a.deleted_at IS NULL AND ({where_sql})
         ORDER BY {RANK_ORDER}
         LIMIT {MAX_CANDIDATES}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| {
        Ok(MetaRow {
            id: r.get(0)?,
            favorite: r.get::<_, i64>(1)? != 0,
            rating: r.get(2)?,
            thumbnail_path: r.get(3)?,
        })
    })?;
    let metas: Vec<MetaRow> = rows.filter_map(|r| r.ok()).collect();
    let ids: Vec<String> = metas.iter().map(|m| m.id.clone()).collect();
    let embeds = load_embeddings(conn, &ids).unwrap_or_default();
    let candidates: Vec<RankCandidate> = metas
        .into_iter()
        .map(|m| RankCandidate {
            id: m.id.clone(),
            favorite: m.favorite,
            rating: m.rating,
            has_thumb: m.thumbnail_path.is_some(),
            embedding: embeds.get(&m.id).cloned(),
        })
        .collect();
    Ok(diversify(candidates, take.max(1)))
}

/// Highest-ranked ids, without the CLIP pass.
///
/// Building a card only needs a cover and a quote, so it must not pay for
/// diversification: that loads an embedding per candidate and compares every
/// candidate against every pick. `list_memory_assets` still diversifies the
/// grid the card opens into, which is where the ordering is actually visible.
///
/// Screenshots are held back: they make a dull cover and their captions read
/// as nonsense quotes ("a screenshot of a website with a list of names"). A
/// memory made only of screenshots still gets one rather than showing nothing.
fn top_ranked_ids(
    conn: &Connection,
    where_sql: &str,
    params: &[&dyn ToSql],
    take: usize,
) -> AppResult<Vec<String>> {
    let not_screenshot = format!(
        "NOT ({})",
        crate::smart::SmartCollection::Screenshots.predicate()
    );
    let ids = ranked_ids(conn, where_sql, Some(&not_screenshot), params, take)?;
    if ids.is_empty() {
        return ranked_ids(conn, where_sql, None, params, take);
    }
    Ok(ids)
}

fn ranked_ids(
    conn: &Connection,
    where_sql: &str,
    extra_sql: Option<&str>,
    params: &[&dyn ToSql],
    take: usize,
) -> AppResult<Vec<String>> {
    let extra = extra_sql.map(|e| format!(" AND ({e})")).unwrap_or_default();
    let sql = format!(
        "SELECT a.id
         FROM assets a
         WHERE a.deleted_at IS NULL AND ({where_sql}){extra}
         ORDER BY {RANK_ORDER}
         LIMIT {take}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| r.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn assets_for_ordered_ids(
    conn: &Connection,
    ordered: &[String],
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let slice: Vec<String> = ordered
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .cloned()
        .collect();
    if slice.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(slice.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT {SELECT_COLUMNS} FROM assets a WHERE a.id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(slice.iter()), map_asset)?;
    let mut by_id = std::collections::HashMap::new();
    for row in rows {
        let asset = row?;
        by_id.insert(asset.id.clone(), asset);
    }
    Ok(slice
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}

fn cover_from_ids(
    conn: &Connection,
    ordered: &[String],
) -> AppResult<(Option<String>, Option<String>)> {
    let Some(id) = ordered.first() else {
        return Ok((None, None));
    };
    let thumb: Option<String> = conn
        .query_row(
            "SELECT thumbnail_path FROM assets WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok((Some(id.clone()), thumb))
}

fn with_quote(conn: &Connection, mut summary: MemorySummary, ordered: &[String]) -> MemorySummary {
    summary.quote = pick_quote(conn, ordered);
    finalize_insight(summary)
}

fn with_cached_prose(conn: &Connection, mut summary: MemorySummary) -> MemorySummary {
    let prompt = prose::build_prompt(
        &summary.title,
        &summary.subtitle,
        summary.quote.as_deref(),
    );
    let hash = prose::input_hash(&prompt);
    if let Ok(Some(p)) = prose::cached_prose(conn, &summary.id, &hash) {
        summary.prose = Some(p);
    }
    finalize_insight(summary)
}

/// Narrative summary for Memories UI (never raw “N photos” alone).
pub fn compose_insight(summary: &MemorySummary) -> String {
    if let Some(prose) = summary
        .prose
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return prose.to_string();
    }
    if let Some(quote) = summary
        .quote
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return match summary.kind {
            MemoryKind::WeekendTrip => {
                if let Some(place) = summary
                    .place_label
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    format!("In {place}: “{quote}”")
                } else {
                    format!("“{quote}”")
                }
            }
            MemoryKind::PersonPlace => match (
                summary
                    .person_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                summary
                    .place_label
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            ) {
                (Some(person), Some(place)) => format!("{person} in {place} — “{quote}”"),
                _ => format!("“{quote}”"),
            },
            MemoryKind::OnThisDay => format!("“{quote}”"),
        };
    }

    match summary.kind {
        MemoryKind::WeekendTrip => {
            let place = summary
                .place_label
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let when = summary
                .subtitle
                .split(" · ")
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.contains("photo"));
            match (place, when) {
                (Some(place), Some(when)) => format!("A weekend in {place}, {when}."),
                (Some(place), None) => format!("A weekend spent in {place}."),
                (None, Some(when)) => format!("A weekend trip from {when}."),
                (None, None) => "A short trip worth remembering.".into(),
            }
        }
        MemoryKind::OnThisDay => "Looking back at this day across the years.".into(),
        MemoryKind::PersonPlace => match (
            summary
                .person_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            summary
                .place_label
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        ) {
            (Some(person), Some(place)) => format!("Moments with {person} in {place}."),
            _ => "People and places that matter.".into(),
        },
    }
}

fn finalize_insight(mut summary: MemorySummary) -> MemorySummary {
    summary.insight = compose_insight(&summary);
    summary
}

/// List curated memories for Home / Discover. Order: On this day, then weekends,
/// then person+place. `limit` caps the total returned. Dismissed ids are omitted.
pub fn list_memories(conn: &Connection, limit: u32) -> AppResult<Vec<MemorySummary>> {
    let limit = limit.clamp(1, 50) as usize;
    let today = Utc::now().date_naive();
    let dismissed = dismissed_ids(conn)?;
    let mut out = Vec::new();

    if let Some(m) = on_this_day_memory(conn, today)? {
        if !dismissed.contains(&m.id) {
            out.push(with_cached_prose(conn, m));
        }
    }
    for m in weekend_trip_memories(conn, today)? {
        if out.len() >= limit {
            break;
        }
        if dismissed.contains(&m.id) {
            continue;
        }
        out.push(with_cached_prose(conn, m));
    }
    if out.len() < limit {
        for m in person_place_memories(conn)? {
            if out.len() >= limit {
                break;
            }
            if dismissed.contains(&m.id) {
                continue;
            }
            out.push(with_cached_prose(conn, m));
        }
    }
    out.truncate(limit);
    Ok(out)
}

fn dismissed_ids(conn: &Connection) -> AppResult<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT memory_id FROM dismissed_memories")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Hide a curated memory from Home / Discover. Photos stay in the library.
pub fn dismiss_memory(conn: &Connection, memory_id: &str) -> AppResult<()> {
    // Validate id shape so we never store garbage.
    let _ = parse_memory_id(memory_id)?;
    let at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO dismissed_memories (memory_id, dismissed_at) VALUES (?1, ?2)",
        params![memory_id, at],
    )?;
    let _ = conn.execute(
        "DELETE FROM memory_prose WHERE memory_id = ?1",
        params![memory_id],
    );
    tracing::info!(memory_id, "memory dismissed");
    Ok(())
}

pub fn get_memory(conn: &Connection, memory_id: &str) -> AppResult<MemoryDetail> {
    let summary = resolve_summary(conn, memory_id)?;
    // Assets are loaded via list_memory_assets — keep this call cheap for open/enrich.
    Ok(MemoryDetail {
        summary: with_cached_prose(conn, summary),
        assets: Vec::new(),
    })
}

pub fn list_memory_assets(
    conn: &Connection,
    memory_id: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let parsed = parse_memory_id(memory_id)?;
    match parsed {
        ParsedId::OnThisDay { month_day } => assets_for_on_this_day(conn, &month_day, limit, offset),
        ParsedId::Weekend { start } => assets_for_weekend(conn, start, limit, offset),
        ParsedId::PersonPlace { person_id, place } => {
            assets_for_person_place(conn, &person_id, &place, limit, offset)
        }
    }
}

/// Create a normal album from a memory's ranked assets. Never called automatically.
pub fn save_memory_as_album(
    conn: &Connection,
    memory_id: &str,
    name: Option<String>,
) -> AppResult<(Album, Vec<String>)> {
    let detail = get_memory(conn, memory_id)?;
    let album_name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| detail.summary.title.clone());
    if album_name.is_empty() {
        return Err(AppError::msg("album name required"));
    }
    let asset_ids: Vec<String> = list_memory_assets(conn, memory_id, MAX_ASSETS_PER_MEMORY, 0)?
        .into_iter()
        .map(|a| a.id)
        .collect();
    if asset_ids.is_empty() {
        return Err(AppError::msg("memory has no photos to save"));
    }

    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let cover = asset_ids.first().cloned();
    conn.execute(
        "INSERT INTO albums (id, name, cover_asset_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, album_name, cover, created_at],
    )?;
    let mut added = 0i64;
    for asset_id in &asset_ids {
        added += conn.execute(
            "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
            params![id, asset_id],
        )? as i64;
    }
    let cover_thumbnail_path: Option<String> = cover.as_ref().and_then(|cid| {
        conn.query_row(
            "SELECT thumbnail_path FROM assets WHERE id = ?1",
            params![cid],
            |r| r.get(0),
        )
        .ok()
        .flatten()
    });
    Ok((
        Album {
            id,
            name: album_name,
            cover_asset_id: cover,
            cover_thumbnail_path,
            created_at,
            asset_count: added,
        },
        asset_ids,
    ))
}

// ─── ID parsing ─────────────────────────────────────────────────────────────

enum ParsedId {
    OnThisDay { month_day: String },
    Weekend { start: NaiveDate },
    PersonPlace { person_id: String, place: String },
}

impl ParsedId {
    fn kind(&self) -> MemoryKind {
        match self {
            Self::OnThisDay { .. } => MemoryKind::OnThisDay,
            Self::Weekend { .. } => MemoryKind::WeekendTrip,
            Self::PersonPlace { .. } => MemoryKind::PersonPlace,
        }
    }
}

fn parse_memory_id(id: &str) -> AppResult<ParsedId> {
    if let Some(rest) = id.strip_prefix("on_this_day:") {
        let month_day = rest.trim();
        if month_day.len() != 5 || month_day.as_bytes().get(2) != Some(&b'-') {
            return Err(AppError::msg("invalid on_this_day memory id"));
        }
        return Ok(ParsedId::OnThisDay {
            month_day: month_day.to_string(),
        });
    }
    if let Some(rest) = id.strip_prefix("weekend:") {
        let start = NaiveDate::parse_from_str(rest.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::msg("invalid weekend memory id"))?;
        return Ok(ParsedId::Weekend { start });
    }
    if let Some(rest) = id.strip_prefix("person_place:") {
        let (person_id, place) = rest
            .split_once('|')
            .ok_or_else(|| AppError::msg("invalid person_place memory id"))?;
        if person_id.is_empty() || place.is_empty() {
            return Err(AppError::msg("invalid person_place memory id"));
        }
        return Ok(ParsedId::PersonPlace {
            person_id: person_id.to_string(),
            place: place.to_string(),
        });
    }
    Err(AppError::msg("unknown memory id"))
}

fn resolve_summary(conn: &Connection, memory_id: &str) -> AppResult<MemorySummary> {
    match parse_memory_id(memory_id)? {
        ParsedId::OnThisDay { month_day } => {
            on_this_day_for_month_day(conn, &month_day)?
                .ok_or_else(|| AppError::msg("memory not found"))
        }
        ParsedId::Weekend { start } => {
            weekend_memory_for_start(conn, start)?.ok_or_else(|| AppError::msg("memory not found"))
        }
        ParsedId::PersonPlace { person_id, place } => person_place_for(conn, &person_id, &place)?
            .ok_or_else(|| AppError::msg("memory not found")),
    }
}

// ─── Templates ──────────────────────────────────────────────────────────────

pub(crate) fn template_on_this_day(year_count: usize, asset_count: i64) -> (String, String) {
    let title = "On this day".to_string();
    let subtitle = if year_count <= 1 {
        format!("{asset_count} photos from past years")
    } else {
        format!("{asset_count} photos across {year_count} years")
    };
    (title, subtitle)
}

pub(crate) fn template_weekend(
    place: Option<&str>,
    start: NaiveDate,
    end: NaiveDate,
    asset_count: i64,
) -> (String, String) {
    let title = match place {
        Some(p) if !p.is_empty() => format!("Weekend in {p}"),
        _ => "Weekend trip".to_string(),
    };
    let subtitle = if start == end {
        format!("{} · {asset_count} photos", start.format("%b %d, %Y"))
    } else {
        format!(
            "{} – {} · {asset_count} photos",
            start.format("%b %d"),
            end.format("%b %d, %Y")
        )
    };
    (title, subtitle)
}

pub(crate) fn template_person_place(person: &str, place: &str, asset_count: i64) -> (String, String) {
    (
        format!("{person} in {place}"),
        format!("{asset_count} photos"),
    )
}

// ─── On this day ────────────────────────────────────────────────────────────

fn on_this_day_memory(conn: &Connection, today: NaiveDate) -> AppResult<Option<MemorySummary>> {
    let month_day = format!("{:02}-{:02}", today.month(), today.day());
    on_this_day_for_month_day(conn, &month_day)
}

fn on_this_day_for_month_day(
    conn: &Connection,
    month_day: &str,
) -> AppResult<Option<MemorySummary>> {
    let this_year = Utc::now().year();
    let mut year_stmt = conn.prepare(
        "SELECT DISTINCT CAST(strftime('%Y', COALESCE(captured_at, created_at)) AS INTEGER)
         FROM assets
         WHERE deleted_at IS NULL
           AND strftime('%m-%d', COALESCE(captured_at, created_at)) = ?1
           AND CAST(strftime('%Y', COALESCE(captured_at, created_at)) AS INTEGER) < ?2",
    )?;
    let years: Vec<i32> = year_stmt
        .query_map(params![month_day, this_year], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    if years.is_empty() {
        return Ok(None);
    }

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets
         WHERE deleted_at IS NULL
           AND strftime('%m-%d', COALESCE(captured_at, created_at)) = ?1
           AND CAST(strftime('%Y', COALESCE(captured_at, created_at)) AS INTEGER) < ?2",
        params![month_day, this_year],
        |r| r.get(0),
    )?;
    if count < MIN_ON_THIS_DAY {
        return Ok(None);
    }

    let ordered = top_ranked_ids(
        conn,
        "strftime('%m-%d', COALESCE(a.captured_at, a.created_at)) = ?1
           AND CAST(strftime('%Y', COALESCE(a.captured_at, a.created_at)) AS INTEGER) < ?2",
        &[&month_day, &this_year],
        SUMMARY_CANDIDATES,
    )?;
    let (cover_asset_id, cover_thumbnail_path) = cover_from_ids(conn, &ordered)?;
    let (title, subtitle) = template_on_this_day(years.len(), count);
    Ok(Some(with_quote(
        conn,
        MemorySummary {
            id: format!("on_this_day:{month_day}"),
            kind: MemoryKind::OnThisDay,
            title,
            subtitle,
            insight: String::new(),
            quote: None,
            prose: None,
            asset_count: count,
            cover_asset_id,
            cover_thumbnail_path,
            start_date: Some(month_day.to_string()),
            end_date: None,
            place_label: None,
            person_name: None,
        },
        &ordered,
    )))
}

fn assets_for_on_this_day(
    conn: &Connection,
    month_day: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let this_year = Utc::now().year();
    let take = (offset as usize)
        .saturating_add(limit as usize)
        .clamp(1, MAX_CANDIDATES);
    let ordered = diversified_ids(
        conn,
        "strftime('%m-%d', COALESCE(a.captured_at, a.created_at)) = ?1
           AND CAST(strftime('%Y', COALESCE(a.captured_at, a.created_at)) AS INTEGER) < ?2",
        &[&month_day, &this_year],
        take,
    )?;
    assets_for_ordered_ids(conn, &ordered, limit, offset)
}

// ─── Weekend trips ──────────────────────────────────────────────────────────

#[derive(Debug)]
struct DayBucket {
    date: NaiveDate,
    count: i64,
}

fn weekend_trip_memories(conn: &Connection, today: NaiveDate) -> AppResult<Vec<MemorySummary>> {
    let earliest = today - Duration::days(365 * 3);
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', COALESCE(captured_at, created_at)) AS d, COUNT(*) AS c
         FROM assets
         WHERE deleted_at IS NULL
           AND COALESCE(captured_at, created_at) >= ?1
           AND COALESCE(captured_at, created_at) < ?2
         GROUP BY d
         HAVING c > 0
         ORDER BY d ASC",
    )?;
    let earliest_s = earliest.format("%Y-%m-%d").to_string();
    let today_s = today.format("%Y-%m-%d").to_string();
    let days: Vec<DayBucket> = stmt
        .query_map(params![earliest_s, today_s], |r| {
            let d: String = r.get(0)?;
            let count: i64 = r.get(1)?;
            Ok((d, count))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(d, count)| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .ok()
                .map(|date| DayBucket { date, count })
        })
        .collect();

    let clusters = find_weekend_clusters(&days);
    let mut out = Vec::new();
    for (start, end, total) in clusters {
        if out.len() >= MAX_WEEKEND_MEMORIES {
            break;
        }
        if let Some(m) = build_weekend_summary(conn, start, end, total)? {
            out.push(m);
        }
    }
    Ok(out)
}

fn range_has_weekend(start: NaiveDate, end: NaiveDate) -> bool {
    let mut d = start;
    while d <= end {
        if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            return true;
        }
        d += Duration::days(1);
    }
    false
}

/// Contiguous day runs of length 2–4 that include Sat or Sun and have enough photos.
fn find_weekend_clusters(days: &[DayBucket]) -> Vec<(NaiveDate, NaiveDate, i64)> {
    if days.is_empty() {
        return Vec::new();
    }
    let mut clusters = Vec::new();
    let mut i = 0;
    while i < days.len() {
        let mut j = i;
        let mut total = days[i].count;
        while j + 1 < days.len() {
            let gap = days[j + 1].date.signed_duration_since(days[j].date).num_days();
            if gap != 1 {
                break;
            }
            let next_len = (j + 1) - i + 1;
            if next_len > 4 {
                break;
            }
            j += 1;
            total += days[j].count;
        }
        let start = days[i].date;
        let end = days[j].date;
        let span = end.signed_duration_since(start).num_days() + 1;
        if (2..=4).contains(&span) && total >= MIN_WEEKEND && range_has_weekend(start, end) {
            clusters.push((start, end, total));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    clusters.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
    clusters
}

fn build_weekend_summary(
    conn: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    total: i64,
) -> AppResult<Option<MemorySummary>> {
    let place = dominant_place_in_range(conn, start, end)?;
    let start_s = start.format("%Y-%m-%d").to_string();
    let end_s = end.format("%Y-%m-%d").to_string();
    let ordered = top_ranked_ids(
        conn,
        "strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) >= ?1
           AND strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) <= ?2",
        &[&start_s, &end_s],
        SUMMARY_CANDIDATES,
    )?;
    let (cover_asset_id, cover_thumbnail_path) = cover_from_ids(conn, &ordered)?;
    let (title, subtitle) = template_weekend(place.as_deref(), start, end, total);
    Ok(Some(with_quote(
        conn,
        MemorySummary {
            id: format!("weekend:{}", start.format("%Y-%m-%d")),
            kind: MemoryKind::WeekendTrip,
            title,
            subtitle,
            insight: String::new(),
            quote: None,
            prose: None,
            asset_count: total,
            cover_asset_id,
            cover_thumbnail_path,
            start_date: Some(start_s),
            end_date: Some(end_s),
            place_label: place,
            person_name: None,
        },
        &ordered,
    )))
}

fn weekend_memory_for_start(
    conn: &Connection,
    start: NaiveDate,
) -> AppResult<Option<MemorySummary>> {
    // Rebuild by scanning a short window from start (up to 4 days).
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', COALESCE(captured_at, created_at)) AS d, COUNT(*) AS c
         FROM assets
         WHERE deleted_at IS NULL
           AND strftime('%Y-%m-%d', COALESCE(captured_at, created_at)) >= ?1
           AND strftime('%Y-%m-%d', COALESCE(captured_at, created_at)) <= ?2
         GROUP BY d
         ORDER BY d ASC",
    )?;
    let end_cap = start + Duration::days(3);
    let days: Vec<DayBucket> = stmt
        .query_map(
            params![
                start.format("%Y-%m-%d").to_string(),
                end_cap.format("%Y-%m-%d").to_string()
            ],
            |r| {
                let d: String = r.get(0)?;
                let count: i64 = r.get(1)?;
                Ok((d, count))
            },
        )?
        .filter_map(|r| r.ok())
        .filter_map(|(d, count)| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .ok()
                .map(|date| DayBucket { date, count })
        })
        .collect();
    let clusters = find_weekend_clusters(&days);
    let Some((s, e, total)) = clusters.into_iter().find(|(s, _, _)| *s == start) else {
        return Ok(None);
    };
    build_weekend_summary(conn, s, e, total)
}

fn dominant_place_in_range(
    conn: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT ap.place_label, COUNT(*) AS c
         FROM assets a
         JOIN asset_places ap ON ap.asset_id = a.id
         WHERE a.deleted_at IS NULL
           AND ap.place_label IS NOT NULL AND TRIM(ap.place_label) != ''
           AND strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) >= ?1
           AND strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) <= ?2
         GROUP BY ap.place_label
         ORDER BY c DESC
         LIMIT 1",
        params![
            start.format("%Y-%m-%d").to_string(),
            end.format("%Y-%m-%d").to_string()
        ],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(Into::into)
}

fn assets_for_weekend(
    conn: &Connection,
    start: NaiveDate,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let Some(summary) = weekend_memory_for_start(conn, start)? else {
        return Ok(Vec::new());
    };
    let end = summary
        .end_date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(start);
    let start_s = start.format("%Y-%m-%d").to_string();
    let end_s = end.format("%Y-%m-%d").to_string();
    let take = (offset as usize)
        .saturating_add(limit as usize)
        .clamp(1, MAX_CANDIDATES);
    let ordered = diversified_ids(
        conn,
        "strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) >= ?1
           AND strftime('%Y-%m-%d', COALESCE(a.captured_at, a.created_at)) <= ?2",
        &[&start_s, &end_s],
        take,
    )?;
    assets_for_ordered_ids(conn, &ordered, limit, offset)
}

// ─── Person + place ─────────────────────────────────────────────────────────

fn person_place_memories(conn: &Connection) -> AppResult<Vec<MemorySummary>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, ap.place_label, COUNT(DISTINCT a.id) AS cnt
         FROM faces f
         JOIN people p ON p.id = f.person_id
           AND p.ignored = 0
           AND p.name IS NOT NULL AND TRIM(p.name) != ''
         JOIN assets a ON a.id = f.asset_id AND a.deleted_at IS NULL
         JOIN asset_places ap ON ap.asset_id = a.id
           AND ap.place_label IS NOT NULL AND TRIM(ap.place_label) != ''
         GROUP BY p.id, ap.place_label
         HAVING cnt >= ?1
         ORDER BY cnt DESC
         LIMIT ?2",
    )?;
    let rows: Vec<(String, String, String, i64)> = stmt
        .query_map(params![MIN_PERSON_PLACE, MAX_PERSON_PLACE_MEMORIES as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut out = Vec::new();
    for (person_id, person_name, place, count) in rows {
        if let Some(m) = person_place_summary(conn, &person_id, &person_name, &place, count)? {
            out.push(m);
        }
    }
    Ok(out)
}

fn person_place_for(
    conn: &Connection,
    person_id: &str,
    place: &str,
) -> AppResult<Option<MemorySummary>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT p.name, COUNT(DISTINCT a.id)
             FROM faces f
             JOIN people p ON p.id = f.person_id AND p.id = ?1
               AND p.ignored = 0
               AND p.name IS NOT NULL AND TRIM(p.name) != ''
             JOIN assets a ON a.id = f.asset_id AND a.deleted_at IS NULL
             JOIN asset_places ap ON ap.asset_id = a.id AND ap.place_label = ?2
             GROUP BY p.id
             HAVING COUNT(DISTINCT a.id) >= ?3",
            params![person_id, place, MIN_PERSON_PLACE],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((person_name, count)) = row else {
        return Ok(None);
    };
    person_place_summary(conn, person_id, &person_name, place, count)
}

fn person_place_summary(
    conn: &Connection,
    person_id: &str,
    person_name: &str,
    place: &str,
    count: i64,
) -> AppResult<Option<MemorySummary>> {
    let ordered = top_ranked_ids(
        conn,
        "EXISTS (SELECT 1 FROM faces f WHERE f.asset_id = a.id AND f.person_id = ?1)
           AND EXISTS (
             SELECT 1 FROM asset_places ap
             WHERE ap.asset_id = a.id AND ap.place_label = ?2
           )",
        &[&person_id, &place],
        SUMMARY_CANDIDATES,
    )?;
    let (cover_asset_id, cover_thumbnail_path) = cover_from_ids(conn, &ordered)?;
    let (title, subtitle) = template_person_place(person_name, place, count);
    Ok(Some(with_quote(
        conn,
        MemorySummary {
            id: format!("person_place:{person_id}|{place}"),
            kind: MemoryKind::PersonPlace,
            title,
            subtitle,
            insight: String::new(),
            quote: None,
            prose: None,
            asset_count: count,
            cover_asset_id,
            cover_thumbnail_path,
            start_date: None,
            end_date: None,
            place_label: Some(place.to_string()),
            person_name: Some(person_name.to_string()),
        },
        &ordered,
    )))
}

fn assets_for_person_place(
    conn: &Connection,
    person_id: &str,
    place: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let take = (offset as usize)
        .saturating_add(limit as usize)
        .clamp(1, MAX_CANDIDATES);
    let ordered = diversified_ids(
        conn,
        "EXISTS (SELECT 1 FROM faces f WHERE f.asset_id = a.id AND f.person_id = ?1)
           AND EXISTS (
             SELECT 1 FROM asset_places ap
             WHERE ap.asset_id = a.id AND ap.place_label = ?2
           )",
        &[&person_id, &place],
        take,
    )?;
    assets_for_ordered_ids(conn, &ordered, limit, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        conn
    }

    fn insert_asset(conn: &Connection, id: &str, captured_at: &str, favorite: bool) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, captured_at, indexed_at, favorite)
             VALUES (?1, ?2, ?3, 'image', ?4, ?4, ?4, ?5)",
            params![
                id,
                format!("/tmp/{id}.jpg"),
                format!("hash-{id}"),
                captured_at,
                if favorite { 1 } else { 0 }
            ],
        )
        .unwrap();
    }

    #[test]
    fn templates_are_deterministic() {
        let (t, s) = template_on_this_day(3, 12);
        assert_eq!(t, "On this day");
        assert!(s.contains("12 photos"));
        assert!(s.contains("3 years"));

        let start = NaiveDate::from_ymd_opt(2024, 7, 27).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 7, 28).unwrap();
        let (t, s) = template_weekend(Some("Lisbon"), start, end, 9);
        assert_eq!(t, "Weekend in Lisbon");
        assert!(s.contains("9 photos"));

        let (t, s) = template_person_place("Ada", "Paris", 7);
        assert_eq!(t, "Ada in Paris");
        assert_eq!(s, "7 photos");
    }

    #[test]
    fn insight_prefers_prose_then_quote_then_warm_template() {
        let base = MemorySummary {
            id: "weekend:2026-01-10".into(),
            kind: MemoryKind::WeekendTrip,
            title: "Weekend in Jhusi, Uttar Pradesh".into(),
            subtitle: "Jan 10 – Jan 13, 2026 · 272 photos".into(),
            insight: String::new(),
            quote: None,
            prose: None,
            asset_count: 272,
            cover_asset_id: None,
            cover_thumbnail_path: None,
            start_date: Some("2026-01-10".into()),
            end_date: Some("2026-01-13".into()),
            place_label: Some("Jhusi, Uttar Pradesh".into()),
            person_name: None,
        };
        let warm = compose_insight(&base);
        assert!(warm.contains("Jhusi"));
        assert!(!warm.contains("272 photos"));

        let mut with_quote = base.clone();
        with_quote.quote = Some("a foggy street at dusk".into());
        assert_eq!(
            compose_insight(&with_quote),
            "In Jhusi, Uttar Pradesh: “a foggy street at dusk”"
        );

        let mut with_prose = with_quote;
        with_prose.prose = Some("Fog and ritual fires marked a winter weekend in Jhusi.".into());
        assert_eq!(
            compose_insight(&with_prose),
            "Fog and ritual fires marked a winter weekend in Jhusi."
        );
    }

    #[test]
    fn weekend_clusters_require_sat_or_sun() {
        let thu = NaiveDate::from_ymd_opt(2024, 7, 25).unwrap(); // Thu
        let fri = NaiveDate::from_ymd_opt(2024, 7, 26).unwrap();
        let sat = NaiveDate::from_ymd_opt(2024, 7, 27).unwrap();
        let days = vec![
            DayBucket {
                date: thu,
                count: 3,
            },
            DayBucket {
                date: fri,
                count: 3,
            },
            DayBucket {
                date: sat,
                count: 3,
            },
        ];
        let clusters = find_weekend_clusters(&days);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].0, thu);
        assert_eq!(clusters[0].1, sat);
        assert_eq!(clusters[0].2, 9);
    }

    #[test]
    fn weekday_only_run_is_not_a_weekend_trip() {
        let mon = NaiveDate::from_ymd_opt(2024, 7, 22).unwrap();
        let tue = NaiveDate::from_ymd_opt(2024, 7, 23).unwrap();
        let wed = NaiveDate::from_ymd_opt(2024, 7, 24).unwrap();
        let days = vec![
            DayBucket {
                date: mon,
                count: 4,
            },
            DayBucket {
                date: tue,
                count: 4,
            },
            DayBucket {
                date: wed,
                count: 4,
            },
        ];
        assert!(find_weekend_clusters(&days).is_empty());
    }

    #[test]
    fn on_this_day_lists_past_years_only() {
        let conn = setup();
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();
        insert_asset(
            &conn,
            "a1",
            &format!("{}-{md}T12:00:00Z", y - 1),
            true,
        );
        insert_asset(
            &conn,
            "a2",
            &format!("{}-{md}T13:00:00Z", y - 2),
            false,
        );
        insert_asset(
            &conn,
            "a3",
            &format!("{}-{md}T14:00:00Z", y - 1),
            false,
        );
        // Same calendar day this year should not count.
        insert_asset(
            &conn,
            "a4",
            &format!("{y}-{md}T10:00:00Z"),
            true,
        );

        let list = list_memories(&conn, 10).unwrap();
        let otd = list
            .iter()
            .find(|m| m.kind == MemoryKind::OnThisDay)
            .expect("on this day");
        assert_eq!(otd.asset_count, 3);
        assert_eq!(otd.id, format!("on_this_day:{md}"));

        let assets = list_memory_assets(&conn, &otd.id, 50, 0).unwrap();
        assert_eq!(assets.len(), 3);
        assert_eq!(assets[0].id, "a1"); // favourite ranks first

        let (album, _) = save_memory_as_album(&conn, &otd.id, None).unwrap();
        assert_eq!(album.name, "On this day");
        assert_eq!(album.asset_count, 3);
    }

    #[test]
    fn on_this_day_uses_caption_as_quote() {
        let conn = setup();
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();
        for i in 0..3 {
            insert_asset(
                &conn,
                &format!("q{i}"),
                &format!("{}-{md}T12:0{i}:00Z", y - 1),
                i == 0,
            );
        }
        conn.execute(
            "INSERT INTO asset_captions (asset_id, caption, model_id, created_at)
             VALUES ('q0', 'A quiet street lined with trees', 'florence-test', datetime('now'))",
            [],
        )
        .unwrap();

        let list = list_memories(&conn, 10).unwrap();
        let otd = list
            .iter()
            .find(|m| m.kind == MemoryKind::OnThisDay)
            .expect("on this day");
        assert_eq!(
            otd.quote.as_deref(),
            Some("A quiet street lined with trees")
        );
    }

    #[test]
    fn clip_diversity_prefers_orthogonal_cover_set() {
        let conn = setup();
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();
        for (id, fav) in [("d0", true), ("d1", true), ("d2", false)] {
            insert_asset(
                &conn,
                id,
                &format!("{}-{md}T12:00:00Z", y - 1),
                fav,
            );
        }
        // d0 and d1 nearly identical; d2 orthogonal.
        let mut a = vec![1.0f32, 0.0];
        crate::ml::vector::normalize(&mut a);
        let mut b = vec![0.99f32, 0.14];
        crate::ml::vector::normalize(&mut b);
        let mut c = vec![0.0f32, 1.0];
        crate::ml::vector::normalize(&mut c);
        for (id, v) in [("d0", a), ("d1", b), ("d2", c)] {
            crate::semantic::store(&conn, id, crate::semantic::IMAGE_MODEL_ID, &v).unwrap();
        }

        let memory_id = format!("on_this_day:{md}");
        let assets = list_memory_assets(&conn, &memory_id, 3, 0).unwrap();
        assert_eq!(assets.len(), 3);
        assert_eq!(assets[0].id, "d0");
        assert_eq!(
            assets[1].id, "d2",
            "second slot should be diverse, not the near-duplicate favourite"
        );
    }

    /// The card path must stay off the CLIP re-ranker: dropping an embedding
    /// row that `diversified_ids` would have loaded must not change the card.
    #[test]
    fn summary_cover_does_not_depend_on_embeddings() {
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();

        let cover_of = |with_embeddings: bool| {
            let conn = setup();
            // "s1" is the near-duplicate favourite that diversity would demote.
            for (id, fav) in [("s0", true), ("s1", true), ("s2", false)] {
                insert_asset(&conn, id, &format!("{}-{md}T12:00:00Z", y - 1), fav);
            }
            if with_embeddings {
                for (id, raw) in [
                    ("s0", vec![1.0f32, 0.0]),
                    ("s1", vec![0.99f32, 0.14]),
                    ("s2", vec![0.0f32, 1.0]),
                ] {
                    let mut v = raw;
                    crate::ml::vector::normalize(&mut v);
                    crate::semantic::store(&conn, id, crate::semantic::IMAGE_MODEL_ID, &v).unwrap();
                }
            }
            let list = list_memories(&conn, 10).unwrap();
            list.into_iter()
                .find(|m| m.kind == MemoryKind::OnThisDay)
                .expect("on this day")
                .cover_asset_id
        };

        assert_eq!(cover_of(true), cover_of(false));
        assert_eq!(cover_of(false).as_deref(), Some("s0"));
    }

    #[test]
    #[ignore]
    fn tmp_time_real_library() {
        let conn = crate::state::open_db(std::path::Path::new(
            "/tmp/lumora-perf/library.db",
        ))
        .unwrap();
        crate::db::migrate(&conn).unwrap();
        let t = std::time::Instant::now();
        let n = cache::rebuild(&conn).unwrap();
        println!("rebuild: {} cards in {:?}", n, t.elapsed());
        let t = std::time::Instant::now();
        let cards = cache::list(&conn, 30).unwrap();
        println!("list: {} cards in {:?}", cards.len(), t.elapsed());
        let t = std::time::Instant::now();
        let built = list_memories(&conn, 30).unwrap();
        println!("uncached build: {} cards in {:?}", built.len(), t.elapsed());
        for c in cards.iter().take(4) {
            println!("  {} | {} | {:?}", c.title, c.insight, c.cover_asset_id);
        }
    }

    #[test]
    fn cover_skips_screenshots_unless_thats_all_there_is() {
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();
        let cover_of = |ids: [(&str, bool); 3]| {
            let conn = setup();
            for (i, (id, fav)) in ids.iter().enumerate() {
                insert_asset(&conn, id, &format!("{}-{md}T12:0{i}:00Z", y - 1), *fav);
            }
            list_memories(&conn, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.kind == MemoryKind::OnThisDay)
                .expect("on this day")
                .cover_asset_id
        };

        // The favourited screenshot outranks both photos, and still loses.
        assert_eq!(
            cover_of([("screenshot-a", true), ("photo-b", false), ("photo-c", false)]).as_deref(),
            Some("photo-c"),
        );
        // A day of nothing but screenshots keeps a cover rather than none.
        assert_eq!(
            cover_of([
                ("screenshot-a", true),
                ("screenshot-b", false),
                ("screenshot-c", false)
            ])
            .as_deref(),
            Some("screenshot-a"),
        );
    }

    #[test]
    fn cache_serves_cards_written_by_rebuild() {
        let conn = setup();
        let today = Utc::now().date_naive();
        let md = format!("{:02}-{:02}", today.month(), today.day());
        let y = today.year();
        for i in 0..3 {
            insert_asset(
                &conn,
                &format!("c{i}"),
                &format!("{}-{md}T12:0{i}:00Z", y - 1),
                i == 0,
            );
        }

        // Nothing built yet: the page opens empty instead of grouping inline.
        assert!(cache::list(&conn, 10).unwrap().is_empty());
        assert!(cache::status(&conn).unwrap().built_at.is_none());

        let built = cache::rebuild(&conn).unwrap();
        assert!(built > 0, "seeded day should group into a card");

        let cards = cache::list(&conn, 10).unwrap();
        assert_eq!(cards.len(), built);
        assert_eq!(cards[0].kind, MemoryKind::OnThisDay);
        assert!(!cards[0].insight.is_empty(), "insight composed on read");

        let status = cache::status(&conn).unwrap();
        assert!(status.built_at.is_some());
        assert_eq!(status.count, built as i64);

        // Dismissing hides a card without paying for another rebuild.
        dismiss_memory(&conn, &cards[0].id).unwrap();
        assert_eq!(cache::list(&conn, 10).unwrap().len(), cards.len() - 1);
    }

    #[test]
    fn person_place_requires_named_person_and_place() {
        let conn = setup();
        for i in 0..5 {
            insert_asset(
                &conn,
                &format!("p{i}"),
                &format!("2023-06-0{}T12:00:00Z", i + 1),
                false,
            );
            conn.execute(
                "INSERT INTO asset_places (asset_id, lat, lon, place_label, country, created_at)
                 VALUES (?1, 48.8, 2.3, 'Paris', 'FR', datetime('now'))",
                params![format!("p{i}")],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO people (id, name, face_count, centroid_count, created_at, updated_at, ignored)
             VALUES ('person-1', 'Ada', 5, 0, datetime('now'), datetime('now'), 0)",
            [],
        )
        .unwrap();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO faces (id, asset_id, person_id, bbox_x, bbox_y, bbox_w, bbox_h, score, embedding, detected_at)
                 VALUES (?1, ?2, 'person-1', 0, 0, 0.1, 0.1, 1.0, X'00', datetime('now'))",
                params![format!("f{i}"), format!("p{i}")],
            )
            .unwrap();
        }

        let list = list_memories(&conn, 20).unwrap();
        let pp = list
            .iter()
            .find(|m| m.kind == MemoryKind::PersonPlace)
            .expect("person place");
        assert_eq!(pp.title, "Ada in Paris");
        assert_eq!(pp.asset_count, 5);
        assert!(pp.id.starts_with("person_place:person-1|"));
    }
}
