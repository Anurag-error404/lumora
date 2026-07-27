import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import {
  api,
  type LibraryFacets,
  type Tag,
  type TagBrowseFilter,
} from "../lib/tauri";
import { idleDefer } from "../lib/idleDefer";
import { labelName } from "../lib/labels";
import type { View } from "../types/app";

/** Tag list, library facets, and the combinable tag/rating/colour filter. */
export function useTagBrowse({
  view,
  setError,
  setSelected,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
  setSelected: Dispatch<SetStateAction<Set<string>>>;
}) {
  const [tags, setTags] = useState<Tag[]>([]);
  const [facets, setFacets] = useState<LibraryFacets | null>(null);
  const [tagBrowse, setTagBrowse] = useState<TagBrowseFilter>({
    tagIds: [],
    ratings: [],
    colorLabels: [],
  });

  const refreshTags = useCallback(async () => {
    try {
      const [rows, facetData] = await Promise.all([
        api.listTags(),
        api.getLibraryFacets(),
      ]);
      setTags(rows);
      setFacets(facetData);
      setTagBrowse((current) => ({
        ...current,
        tagIds: current.tagIds.filter((id) =>
          rows.some((tag) => tag.id === id),
        ),
      }));
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => idleDefer(() => void refreshTags()), [refreshTags]);

  useEffect(() => {
    if (view === "tags") void refreshTags();
  }, [view, refreshTags]);

  const toggleTagFilter = useCallback(
    (tagId: string) => {
      setTagBrowse((prev) => {
        const has = prev.tagIds.includes(tagId);
        return {
          ...prev,
          tagIds: has
            ? prev.tagIds.filter((id) => id !== tagId)
            : [...prev.tagIds, tagId],
        };
      });
      setSelected(new Set());
    },
    [setSelected],
  );

  const toggleRatingFilter = useCallback(
    (rating: number) => {
      setTagBrowse((prev) => {
        const has = prev.ratings.includes(rating);
        return {
          ...prev,
          ratings: has
            ? prev.ratings.filter((r) => r !== rating)
            : [...prev.ratings, rating].sort((a, b) => b - a),
        };
      });
      setSelected(new Set());
    },
    [setSelected],
  );

  const toggleColorFilter = useCallback(
    (color: string) => {
      setTagBrowse((prev) => {
        const has = prev.colorLabels.includes(color);
        return {
          ...prev,
          colorLabels: has
            ? prev.colorLabels.filter((c) => c !== color)
            : [...prev.colorLabels, color],
        };
      });
      setSelected(new Set());
    },
    [setSelected],
  );

  const clearTagBrowse = useCallback(() => {
    setTagBrowse({ tagIds: [], ratings: [], colorLabels: [] });
    setSelected(new Set());
  }, [setSelected]);

  const tagBrowseActive =
    tagBrowse.tagIds.length > 0 ||
    tagBrowse.ratings.length > 0 ||
    tagBrowse.colorLabels.length > 0;

  const ratingCounts = useMemo(() => {
    const map = new Map<number, number>();
    for (const facet of facets?.ratings ?? []) {
      map.set(Number(facet.value), facet.count);
    }
    return map;
  }, [facets]);

  const colorCounts = useMemo(() => {
    const map = new Map<string, number>();
    for (const facet of facets?.colorLabels ?? []) {
      map.set(facet.value, facet.count);
    }
    return map;
  }, [facets]);

  const tagBrowseSummary = useMemo(() => {
    const parts: string[] = [];
    for (const id of tagBrowse.tagIds) {
      const tag = tags.find((t) => t.id === id);
      if (tag) parts.push(tag.name);
    }
    for (const rating of [...tagBrowse.ratings].sort((a, b) => b - a)) {
      parts.push(`${"★".repeat(rating)}`);
    }
    for (const color of tagBrowse.colorLabels) {
      parts.push(labelName(color));
    }
    return parts;
  }, [tagBrowse, tags]);

  return {
    tags,
    tagBrowse,
    tagBrowseActive,
    ratingCounts,
    colorCounts,
    tagBrowseSummary,
    refreshTags,
    toggleTagFilter,
    toggleRatingFilter,
    toggleColorFilter,
    clearTagBrowse,
  };
}
