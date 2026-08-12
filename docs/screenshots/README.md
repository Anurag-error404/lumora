# Screenshots & demo GIFs

Marketing assets used by the [README](../../README.md) and site pages.

Raw captures are `.png`; the site and README reference the `.webp` variants
(3000 px retina screenshots are 4–5 MB each, the WebP builds are 40–170 KB).
Only `*.webp` is published to GitHub Pages. After adding or re-taking a capture,
rebuild its WebP:

```bash
# heroes 1920 px wide, everything else 1600 px
cwebp -q 78 -resize 1920 0 hero_search_sunset.png -o hero_search_sunset.webp
cwebp -q 80 -resize 1600 0 timeline.png -o timeline.webp
```

## Current screenshots

| File | Subject | Used on site |
| --- | --- | --- |
| `hero_search_black_dog` | Full-bleed results grid — “black dog” | Homepage hero rotation |
| `hero_search_sunset` | Full-bleed results grid — “sunset” | Homepage hero rotation |
| `hero_search_mountain_trail` | Full-bleed results grid — “mountain trail” | Homepage hero rotation |
| `home` | Home / library grid | Demo cycle, library band |
| `search-black-dog` | Semantic search — “black dog” | Demo cycle, playground, Mylio page |
| `search-bird-on-tree` | Semantic search — “Bird on tree” | Playground, offline search page |
| `search-nature-sunset` | Semantic search — “nature sunset” | Playground, search band |
| `search_mountain_hike` | Semantic search — “mountain hike” | Demo cycle, playground |
| `search_reciept_from_2026` | OCR search — “receipt from 2026” | Playground |
| `search_sports_car` | Semantic search — “sports car” | Playground |
| `info-panel` | Viewer details — auto-tags + caption | README |
| `duplicates` | Duplicate & blurry cleanup | Cleanup band |
| `places` | Offline Places map | PhotoPrism alternative |
| `locked_folder` | Locked folder unlock (encrypted vault) | README, vault band |
| `timeline` | Timeline by capture date | Memories section, digiKam alternative |
| `edit-photo` | Photo editor (crop / exposure) | README |
| `ai-settings` | Settings → AI Features | Intelligence band, Immich alternative |
| `plugins` | Plugins gallery | Plugins band, plugin guide gallery |

Playground chips and hero captions must use screenshots of the **actual query** —
the label text and the search field in the image have to match.

## Nice to add later

Short demo GIFs (5–10 s, &lt; 5 MB each): `semantic-search.gif`, `ocr-search.gif`, `duplicates.gif`, `faces.gif`. Wire new files into `README.md` under **What you can do** and replace the homepage demo cycle when ready.
