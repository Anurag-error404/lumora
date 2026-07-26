If you're building a **local-first Google Photos alternative**, I'd optimise for **performance, privacy, and AI on-device** rather than a web-first architecture.

Since this app will be indexing potentially **100k–1M images/videos**, the architecture matters much more than the UI framework.

---

# Recommended Tech Stack

## Desktop

I would build it using:

| Layer            | Recommendation           | Why                        |
| ---------------- | ------------------------ | -------------------------- |
| UI               | React + TypeScript       | Huge ecosystem             |
| Desktop          | Tauri v2                 | Tiny binary, Rust backend  |
| Backend          | Rust                     | Extremely fast indexing    |
| Database         | SQLite                   | Perfect local database     |
| Search           | SQLite FTS5              | Fast metadata search       |
| ML               | ONNX Runtime             | Local AI inference         |
| Image Processing | OpenCV + image crate     | Face detection, thumbnails |
| Face Recognition | InsightFace ONNX         | Excellent accuracy         |
| Object Detection | YOLOv11 Nano / MobileSAM | Fast tagging               |
| Embeddings       | CLIP ViT-B32             | Semantic search            |
| File Watching    | notify (Rust)            | Live updates               |
| Metadata         | exiftool / rexiv2        | EXIF editing               |
| Encryption       | SQLCipher + AES256       | Locked folder              |

---

# Why Tauri instead of Electron?

Electron:

* 300MB RAM idle
* Chromium
* Node

Tauri

* ~20MB RAM
* Native
* Rust backend
* Faster filesystem access

For a photo manager, filesystem performance is everything.

---

# Overall Architecture

```
               UI (React)

                   │

             Tauri Commands

                   │

      ┌─────────────────────────┐
      │       Rust Core         │
      ├─────────────────────────┤
      │ File Scanner            │
      │ Thumbnail Generator     │
      │ Face Recognition        │
      │ Embeddings              │
      │ Tagging                 │
      │ Search                  │
      │ Encryption              │
      └─────────────────────────┘

                   │

              SQLite Database

                   │

          Original Photos (Disk)

```

---

# AI Models

## Face Detection

InsightFace

Pros

* state of the art
* ONNX available
* works offline

Outputs

```
Face ID

Bounding Box

Embedding

Age

Gender (optional)

Pose
```

---

## Image Embeddings

Use CLIP

Allows searches like

> dog on beach

> sunset

> me in mountains

> red dress

without explicit tags.

---

## Object Recognition

YOLO Nano

Can auto-tag

```
Dog

Car

Laptop

Mountain

Food

Passport

Receipt

Cat

Beach

```

---

# Folder Structure

```
src/

frontend/

backend/

ml/

database/

thumbnail/

indexer/

watcher/

search/

privacy/

sync/

```

---

# Database

Images

```
Image

id

path

hash

width

height

created_at

favorite

hidden

locked

rating

thumbnail

embedding_id
```

Albums

```
Album

id

name

cover

created_at
```

Album Images

```
album_id

image_id
```

Tags

```
Tag

id

name
```

Image Tags

```
image_id

tag_id
```

Faces

```
Face

id

person_id

image_id

x

y

w

h

embedding
```

People

```
Person

id

name

cover_face
```

---

# Background Indexer

Runs continuously

```
Watch folders

↓

Hash image

↓

Read EXIF

↓

Generate thumbnail

↓

Generate CLIP embedding

↓

Detect faces

↓

Run YOLO tagging

↓

Store everything
```

---

# Search Features

Normal Search

```
beach

cat

food

birthday
```

Natural Search

```
me wearing glasses

sunset near lake

snow vacation

passport

red car

```

Filter Search

```
camera:iphone

rating>4

person:John

before:2021

after:2023

lens:50mm

location:Delhi
```

---

# Privacy Folder

Requirements

Move files into encrypted vault.

Features

```
AES256 encryption

Password

Biometric (Windows Hello/macOS Touch ID where supported)

Auto lock

No thumbnails outside vault

No indexing

Export

```

---

# Duplicate Detection

Exact

SHA256

Near Duplicate

Perceptual hash

Find

```
Screenshots

Burst photos

Edited copies

Compressed copies

```

---

# Smart Albums

Automatically generated

```
Today

Last Week

Videos

Favorites

Documents

Pets

Food

Travel

People

Screenshots

Portraits

RAW

Recently Added

```

---

# Timeline

```
2026

July

June

May

```

Google Photos style.

---

# Map View

Using EXIF GPS

```
Cluster images

Travel history

Heat map
```

---

# Editing

Basic

```
Crop

Rotate

Brightness

Contrast

Saturation

Blur

Sharpen

```

---

# AI Features

## Face Clustering

Automatically groups

```
Unknown Person

↓

Merge

↓

Name

↓

Future photos auto classified
```

---

## OCR

Using PaddleOCR

Extract

```
Receipts

Notes

Whiteboards

Passports

Business Cards
```

---

## Semantic Search

Instead of

```
Tag: dog
```

Use embeddings

```
"dog playing with child"

```

Works automatically.

---

# Nice Quality-of-Life Features

### Favorites

⭐

---

### Ratings

1–5 stars

---

### Color Labels

Like Lightroom

---

### Smart Collections

```
Edited

Unedited

RAW

HDR

```

---

### Bulk Rename

---

### Duplicate Cleaner

---

### Similar Photos

Like Apple Photos.

---

### Compare Mode

Side-by-side.

---

### Drag-drop Albums

---

### Keyboard Shortcuts

Space

Fullscreen

Arrow navigation

Delete

Favorite

Tag

---

### Undo Everything

Every destructive operation reversible.

---

### Trash

30 days.

---

### Version History

If edited.

---

### Face Merge

Merge

```
John

Johnny

John Smith

```

into one person.

---

### AI Captioning

Generate

```
"A family enjoying sunset on the beach."
```

---

### AI Story

Generate vacation memories.

---

### Slideshow

---

### Dark Mode

---

### Plugin Support

Future

```
Image editors

Cloud sync

Exporters
```

---

# Future Features

* Optional end-to-end encrypted sync between devices
* Shared albums over LAN
* Mobile companion app
* Live Photos/Motion Photos support
* AI-powered image enhancement and restoration
* Local generative search ("Show all photos where Sarah is smiling at the beach")
* Automatic event detection (birthdays, trips, concerts)

---

# Performance Targets

| Metric                     | Goal                                                       |
| -------------------------- | ---------------------------------------------------------- |
| Cold startup               | <2 seconds                                                 |
| Search latency             | <100 ms                                                    |
| Thumbnail generation       | 100 images/minute (background)                             |
| Semantic search            | <500 ms over 1 million images                              |
| Face recognition           | >95% precision on clear faces                              |
| RAM usage (idle)           | <250 MB                                                    |
| Database size              | <10 GB metadata for 1 million photos (excluding originals) |
| Background indexing impact | Low priority with CPU throttling when system is busy       |

---

# Product Requirements Document (PRD)

## 1. Product Overview

**Working Title:** PhotoVault AI (placeholder)

**Vision:** Build a fast, privacy-first, local photo management application that organises, enriches, and helps users rediscover their memories entirely on-device. The application should deliver Google Photos-like convenience without requiring cloud uploads.

### Goals

* Deliver a seamless local photo library experience.
* Provide AI-powered organisation without sending data to external servers.
* Support very large libraries (up to 1 million assets).
* Make advanced features (face recognition, semantic search, OCR, duplicate detection) available offline.

### Non-Goals (v1)

* Cloud backup.
* Multi-user collaboration.
* Full professional RAW editing.
* Social sharing platform.

## 2. Target Users

* Photography enthusiasts.
* Privacy-conscious users.
* Families with large local photo collections.
* Professionals managing image archives.
* Users migrating away from cloud photo services.

## 3. Core User Stories

* Import photos from folders or drives.
* Automatically organise media by date, people, places, and objects.
* Find photos using keywords or natural language.
* Group images into albums manually or automatically.
* Lock sensitive photos in an encrypted vault.
* Mark favourites and assign ratings.
* Detect duplicates and free storage safely.
* Search text inside images via OCR.

## 4. Functional Requirements

### Library Management

* Import existing folders.
* Watch folders for changes.
* Preserve originals.
* Incremental indexing.
* Duplicate detection.

### Metadata

* Read/write EXIF and IPTC where appropriate.
* Custom tags.
* Ratings.
* Colour labels.
* Favourite flag.

### AI Features

* Face detection and clustering.
* Person naming and merging.
* Object recognition.
* Semantic embeddings.
* OCR.
* Smart album generation.

### Search

* Text search.
* Metadata filters.
* Semantic search.
* Saved searches.

### Privacy

* Encrypted locked folder.
* Password or platform biometric unlock.
* Automatic locking.
* Exclude vault contents from indexing unless unlocked.

### Organisation

* Albums.
* Smart albums.
* Timeline.
* Map view.
* Recently added.
* Recently viewed.

### Editing (Basic)

* Crop.
* Rotate.
* Exposure adjustments.
* Non-destructive edit history (stretch goal for v1.5).

## 5. Non-Functional Requirements

* Offline-first.
* Cross-platform (Windows, macOS, Linux).
* Responsive UI with virtualised grids.
* Background tasks should not noticeably impact interactive performance.
* Database integrity checks and recovery.

## 6. Success Metrics

* Initial indexing success rate >99%.
* Search response <100 ms for indexed metadata.
* User can locate a photo within 10 seconds using search or filters.
* Zero external network dependency for core features.
* Stable handling of libraries containing 500k+ assets.

## 7. Risks

* Large-scale face recognition can be CPU-intensive; background scheduling and incremental processing are essential.
* Model updates may require reprocessing embeddings.
* Cross-platform filesystem quirks (symbolic links, network drives, permissions) need careful handling.
* Secure key management for the locked vault is critical to avoid data loss.

## 8. Roadmap

**Phase 1 (MVP):** Library import, thumbnails, metadata, albums, favourites, tags, search, timeline, duplicate detection.

**Phase 2:** Face recognition, semantic search, OCR, smart albums, map view, encrypted vault.

**Phase 3:** Advanced editing, plugin system, optional encrypted sync, mobile companion, AI-generated memories and stories.

This stack and roadmap provide a strong foundation for a high-performance, local-first application that can scale from a personal photo collection to a professional-grade archive while keeping user data private.
