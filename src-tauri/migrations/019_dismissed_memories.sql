-- Dismissed curated memories.
--
-- Memories are computed on the fly (no story table). Dismissing a memory_id
-- hides it from Home / Discover until the user clears dismissals.

CREATE TABLE IF NOT EXISTS dismissed_memories (
    memory_id TEXT PRIMARY KEY NOT NULL,
    dismissed_at TEXT NOT NULL
);
