import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type AssetSummary, type TimelineMonth } from "../lib/tauri";
import { MONTHS } from "../lib/constants";
import type { TimelineYearGroup, View } from "../types/app";

/** Timeline months, lazily loaded month assets, and infinite scroll. */
export function useTimeline({
  view,
  setError,
  selected,
  setSelected,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
  selected: Set<string>;
  setSelected: Dispatch<SetStateAction<Set<string>>>;
}) {
  const [timeline, setTimeline] = useState<TimelineMonth[]>([]);
  const [timelineAssets, setTimelineAssets] = useState<
    Record<string, AssetSummary[]>
  >({});
  const [timelineVisibleCount, setTimelineVisibleCount] = useState(6);
  const [timelineLoading, setTimelineLoading] = useState(false);
  const timelineSentinelRef = useRef<HTMLDivElement>(null);

  const timelineKey = useCallback(
    (month: TimelineMonth) => `${month.year}-${month.month}`,
    [],
  );

  useEffect(() => {
    if (view === "timeline") {
      void api
        .timelineMonths()
        .then((m) => {
          setTimeline(m);
          setTimelineVisibleCount(Math.min(6, m.length));
        })
        .catch((e) => setError(String(e)));
    }
  }, [view, setError]);

  useEffect(() => {
    if (view !== "timeline" || timeline.length === 0) return;
    const visible = timeline.slice(0, timelineVisibleCount);
    const missing = visible.filter((month) => !timelineAssets[timelineKey(month)]);
    if (missing.length === 0) return;

    let cancelled = false;
    setTimelineLoading(true);
    void Promise.all(
      missing.map(async (month) => ({
        key: timelineKey(month),
        rows: await api.listAssetsForMonth(month.year, month.month, 5000, 0),
      })),
    )
      .then((groups) => {
        if (cancelled) return;
        setTimelineAssets((current) => {
          const next = { ...current };
          for (const group of groups) next[group.key] = group.rows;
          return next;
        });
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setTimelineLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [
    view,
    timeline,
    timelineVisibleCount,
    timelineAssets,
    timelineKey,
    setError,
  ]);

  useEffect(() => {
    if (view !== "timeline") return;
    const target = timelineSentinelRef.current;
    if (!target) return;
    const root = target.closest(".content");
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setTimelineVisibleCount((count) =>
            Math.min(count + 6, timeline.length),
          );
        }
      },
      { root, rootMargin: "300px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [view, timeline.length, timelineVisibleCount]);

  const visibleTimelineMonths = useMemo(
    () => timeline.slice(0, timelineVisibleCount),
    [timeline, timelineVisibleCount],
  );

  /** Distinct years across the full timeline (not just what's loaded yet). */
  const timelineScaleYears = useMemo(() => {
    const years: number[] = [];
    for (const month of timeline) {
      if (years[years.length - 1] !== month.year) years.push(month.year);
    }
    return years;
  }, [timeline]);

  const timelineYears = useMemo<TimelineYearGroup[]>(() => {
    const groups: TimelineYearGroup[] = [];
    for (const month of visibleTimelineMonths) {
      const current = groups[groups.length - 1];
      if (!current || current.year !== month.year) {
        groups.push({ year: month.year, months: [month] });
      } else {
        current.months.push(month);
      }
    }
    return groups;
  }, [visibleTimelineMonths]);

  const visibleTimelineAssetCount = useMemo(
    () =>
      visibleTimelineMonths.reduce(
        (count, month) =>
          count + (timelineAssets[timelineKey(month)]?.length ?? 0),
        0,
      ),
    [visibleTimelineMonths, timelineAssets, timelineKey],
  );

  const timelineFlatAssets = useMemo(() => {
    const rows: AssetSummary[] = [];
    for (const year of timelineYears) {
      for (const month of year.months) {
        rows.push(...(timelineAssets[timelineKey(month)] ?? []));
      }
    }
    return rows;
  }, [timelineYears, timelineAssets, timelineKey]);

  /**
   * Expand the lazy window far enough to include `year`, then return true so
   * the view can scroll to that year's heading once it mounts.
   */
  const jumpToYear = useCallback(
    (year: number) => {
      let lastIndex = -1;
      for (let i = 0; i < timeline.length; i++) {
        if (timeline[i].year === year) lastIndex = i;
      }
      if (lastIndex < 0) return false;
      setTimelineVisibleCount((count) => Math.max(count, lastIndex + 1));
      return true;
    },
    [timeline],
  );

  async function selectTimelineGroup(m: TimelineMonth) {
    try {
      const key = timelineKey(m);
      const rows =
        timelineAssets[key] ??
        (await api.listAssetsForMonth(m.year, m.month, 5000, 0));
      setTimelineAssets((current) => ({ ...current, [key]: rows }));
      const ids = rows.map((row) => row.id);
      const monthLabel = `${MONTHS[m.month - 1]} ${m.year}`;
      const allSelected =
        ids.length > 0 && ids.every((id) => selected.has(id));

      setSelected((current) => {
        const next = new Set(current);
        if (allSelected) {
          for (const id of ids) next.delete(id);
        } else {
          for (const id of ids) next.add(id);
        }
        return next;
      });
      setError(
        allSelected
          ? `Cleared selection for ${monthLabel}`
          : `Selected ${ids.length} photo(s) from ${monthLabel}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  return {
    timeline,
    timelineAssets,
    setTimelineAssets,
    timelineVisibleCount,
    timelineLoading,
    timelineSentinelRef,
    timelineKey,
    visibleTimelineMonths,
    timelineYears,
    timelineScaleYears,
    visibleTimelineAssetCount,
    timelineFlatAssets,
    jumpToYear,
    selectTimelineGroup,
  };
}
