# Implementation Plan: Memories v1.5

## Overview

Upgrade Memories with **CLIP diversity ranking** (when embeddings exist) and
**Florence captions as quotes** on memory cards. Still offline; no new models.

## Architecture Decisions

- **Graceful degrade** — no CLIP ⇒ v1 favourite/rating order; no captions ⇒ no quote.
- **Greedy diversity** — seed by base score, then pick `base − λ · max_sim(selected)`.
- **Quote field** — `MemorySummary.quote` separate from subtitle (metadata stays).
- **Candidate cap** — re-rank at most 400 candidates per memory for latency.

## Tasks

- [ ] Rank helper + wire into asset lists / covers
- [ ] Caption quote picker + MemorySummary.quote
- [ ] UI: show quote on cards + detail
- [ ] Tests with synthetic CLIP vectors + captions
- [ ] SPEC checklist v1.5 shipped
