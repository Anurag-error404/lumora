import { describe, expect, test } from "bun:test";
import { mergeSearchResults } from "../src/features/search/merge-search-results";
import type { AssetSummary } from "../src/lib/tauri";

function asset(id: string, path: string): AssetSummary {
  return {
    id,
    path,
    hash: id,
    perceptualHash: null,
    mediaType: "image",
    width: 1,
    height: 1,
    durationMs: null,
    createdAt: "2026-01-01",
    capturedAt: null,
    indexedAt: "2026-01-01",
    favorite: false,
    rating: 0,
    colorLabel: null,
    thumbnailPath: null,
    camera: null,
    lens: null,
    deletedAt: null,
  };
}

describe("mergeSearchResults", () => {
  test("puts FTS / OCR hits before semantic and dedupes", () => {
    const fts = [asset("ocr-hit", "/a.jpg"), asset("both", "/b.jpg")];
    const semantic = [asset("both", "/b.jpg"), asset("clip-only", "/c.jpg")];
    const merged = mergeSearchResults(fts, semantic, 10);
    expect(merged.map((a) => a.id)).toEqual(["ocr-hit", "both", "clip-only"]);
  });

  test("respects limit", () => {
    const fts = [asset("1", "/1.jpg"), asset("2", "/2.jpg")];
    const semantic = [asset("3", "/3.jpg")];
    expect(mergeSearchResults(fts, semantic, 2).map((a) => a.id)).toEqual([
      "1",
      "2",
    ]);
  });
});
