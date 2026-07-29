# Implementation Plan: Memories v1

## Overview

Local-only “Memories” curated from existing library data (dates, people, places, favourites). v1 uses **SQL clustering + simple ranking + caption/metadata templates**. No new models, no network, no auto-created albums. Users can optionally **Save as album**.

## Architecture Decisions

- **Ephemeral compute** — no `memories` table in v1; `list_memories` / `get_memory` run over assets + faces + places.
- **Deterministic IDs** — `on_this_day:MM-DD`, `weekend:YYYY-MM-DD`, `person_place:{personId}:{placeLabel}` so detail + save-as-album resolve without persistence.
- **Templates only** — titles/subtitles from structured slots (date, place, person, counts). Captions unused until v1.5.
- **Ranking v1** — favourites + rating + thumbnail presence (CLIP diversity deferred to v1.5).
- **UI** — Discover → Memories sidebar; Home strip of top memories; detail reuses library grid; Save as album → existing `create_album_with_assets`.

## Task List

### Phase 1: Contract + On this day
- [ ] Task 1: SPEC + types + `list_memories` / `get_memory` / `save_memory_as_album` stubs
- [ ] Task 2: On this day generator + unit tests
- [ ] Task 3: Memories view + sidebar + Home strip (On this day only)

### Checkpoint: On this day end-to-end
- [ ] Tests pass; typecheck; can open memory and save as album

### Phase 2: Remaining generators
- [ ] Task 4: Weekend trips
- [ ] Task 5: Person + place
- [ ] Task 6: Polish empty states / ordering / SPEC checklist

### Checkpoint: Complete
- [ ] All three kinds surface when data exists
- [ ] No auto albums created

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Large libraries slow `list_memories` | Med | Cap candidates; SQL date filters; limit person×place pairs |
| Sparse GPS/faces → empty Memories | Low | Empty state explains need for dates / Places / named people |
| Ambiguous weekend boundaries | Low | Fri–Mon clusters with ≥5 photos; dominant place label |

## Open Questions

None blocking — product home confirmed (Home + Discover sidebar; save-as-album optional).
