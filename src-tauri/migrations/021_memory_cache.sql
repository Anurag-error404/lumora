-- Cached memory cards.
--
-- Grouping a memory (day buckets, weekend clusters, person+place pairs) scans
-- the whole library and costs seconds. A background builder does that work and
-- writes the finished cards here; the Memories UI only ever reads this table,
-- so opening the page is a single indexed select.

CREATE TABLE IF NOT EXISTS memory_cards (
    id TEXT PRIMARY KEY NOT NULL,
    position INTEGER NOT NULL,
    title TEXT NOT NULL,
    subtitle TEXT NOT NULL,
    quote TEXT,
    asset_count INTEGER NOT NULL,
    cover_asset_id TEXT,
    cover_thumbnail_path TEXT,
    start_date TEXT,
    end_date TEXT,
    place_label TEXT,
    person_name TEXT
);

CREATE INDEX IF NOT EXISTS idx_memory_cards_position ON memory_cards(position);

-- Single row: when the cards above were last built. Kept separate from the
-- cards so "built, found nothing" is distinguishable from "never built" —
-- the UI shows a loader only for the latter.
CREATE TABLE IF NOT EXISTS memory_cache_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    built_at TEXT NOT NULL,
    built_on TEXT NOT NULL
);
