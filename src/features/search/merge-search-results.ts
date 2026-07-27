import type { AssetSummary } from "../lib/tauri";

/**
 * Prefer exact FTS hits (filename, tags, OCR text, people, auto-tags) and fill
 * the rest with CLIP semantic results. Dedupes by asset id.
 */
export function mergeSearchResults(
  fts: AssetSummary[],
  semantic: AssetSummary[],
  limit: number,
): AssetSummary[] {
  const seen = new Set<string>();
  const out: AssetSummary[] = [];

  for (const asset of fts) {
    if (seen.has(asset.id)) continue;
    seen.add(asset.id);
    out.push(asset);
    if (out.length >= limit) return out;
  }

  for (const asset of semantic) {
    if (seen.has(asset.id)) continue;
    seen.add(asset.id);
    out.push(asset);
    if (out.length >= limit) return out;
  }

  return out;
}
