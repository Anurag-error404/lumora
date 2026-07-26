import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent as ReactDragEvent,
  type RefObject,
} from "react";
import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MONTHS } from "../../lib/constants";
import type { AssetSummary, TimelineMonth } from "../../lib/tauri";
import type { TimelineYearGroup } from "../../types/app";
import { AssetCard } from "../library/AssetCard";

type DayGroup = {
  key: string;
  day: number;
  label: string;
  assets: AssetSummary[];
};

function dayFromAsset(asset: AssetSummary): {
  day: number;
  key: string;
  label: string;
} {
  const raw = asset.capturedAt || asset.createdAt;
  // EXIF dates are often "2024-07-26 …" or "2024:07:26 …"
  const normalized = raw.replace(/:/g, "-");
  const match = normalized.match(/(\d{4})-(\d{2})-(\d{2})/);
  if (!match) {
    return { day: 0, key: "unknown", label: "Unknown date" };
  }
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  const weekday = date.toLocaleDateString(undefined, { weekday: "short" });
  return {
    day,
    key: `${year}-${month}-${day}`,
    label: `${weekday} ${day}`,
  };
}

function groupAssetsByDay(assets: AssetSummary[]): DayGroup[] {
  const map = new Map<string, DayGroup>();
  for (const asset of assets) {
    const { day, key, label } = dayFromAsset(asset);
    const existing = map.get(key);
    if (existing) {
      existing.assets.push(asset);
    } else {
      map.set(key, { key, day, label, assets: [asset] });
    }
  }
  return Array.from(map.values()).sort((a, b) => b.day - a.day);
}

/** Chronological feed grouped by year → month → day with a year/month scrubber. */
export function TimelineView({
  timelineYears,
  timelineScaleYears,
  timeline,
  timelineAssets,
  timelineKey,
  timelineLoading,
  timelineVisibleCount,
  timelineTotal,
  sentinelRef,
  selected,
  onJumpToYear,
  onSelectMonth,
  onImport,
  onAssetDragStart,
  onAssetDragEnd,
  onToggleSelect,
  onOpen,
  onToggleFavorite,
  onShowInfo,
}: {
  timelineYears: TimelineYearGroup[];
  timelineScaleYears: number[];
  timeline: TimelineMonth[];
  timelineAssets: Record<string, AssetSummary[]>;
  timelineKey: (month: TimelineMonth) => string;
  timelineLoading: boolean;
  timelineVisibleCount: number;
  timelineTotal: number;
  sentinelRef: RefObject<HTMLDivElement | null>;
  selected: Set<string>;
  onJumpToYear: (year: number) => boolean;
  onSelectMonth: (month: TimelineMonth) => void;
  onImport: () => void;
  onAssetDragStart: (
    e: ReactDragEvent<HTMLDivElement>,
    asset: AssetSummary,
  ) => void;
  onAssetDragEnd: () => void;
  onToggleSelect: (id: string) => void;
  onOpen: (id: string) => void;
  onToggleFavorite: (asset: AssetSummary) => void;
  onShowInfo: (id: string) => void;
}) {
  const [activeYear, setActiveYear] = useState<number | null>(
    timelineScaleYears[0] ?? null,
  );
  const [pendingYear, setPendingYear] = useState<number | null>(null);
  const [pendingMonth, setPendingMonth] = useState<TimelineMonth | null>(null);
  const yearRefs = useRef(new Map<number, HTMLElement>());
  const monthRefs = useRef(new Map<string, HTMLElement>());

  const monthsForActiveYear = useMemo(
    () => timeline.filter((m) => m.year === activeYear),
    [timeline, activeYear],
  );

  // Track which year is in view while scrolling.
  useEffect(() => {
    if (timelineYears.length === 0) return;
    const root = sentinelRef.current?.closest(".content") ?? null;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort(
            (a, b) =>
              a.boundingClientRect.top - b.boundingClientRect.top,
          );
        const top = visible[0]?.target.getAttribute("data-year");
        if (top) setActiveYear(Number(top));
      },
      { root, rootMargin: "-15% 0px -55% 0px", threshold: [0, 0.1, 0.4] },
    );
    for (const year of timelineYears) {
      const node = yearRefs.current.get(year.year);
      if (node) observer.observe(node);
    }
    return () => observer.disconnect();
  }, [timelineYears, sentinelRef]);

  // After jump expands the lazy window, scroll to the year or month heading.
  useEffect(() => {
    if (pendingMonth) {
      const key = timelineKey(pendingMonth);
      const node = monthRefs.current.get(key);
      if (!node) return;
      node.scrollIntoView({ behavior: "smooth", block: "start" });
      setActiveYear(pendingMonth.year);
      setPendingMonth(null);
      setPendingYear(null);
      return;
    }
    if (pendingYear == null) return;
    const node = yearRefs.current.get(pendingYear);
    if (!node) return;
    node.scrollIntoView({ behavior: "smooth", block: "start" });
    setActiveYear(pendingYear);
    setPendingYear(null);
  }, [pendingYear, pendingMonth, timelineYears, timelineKey]);

  function handleJumpYear(year: number) {
    if (!onJumpToYear(year)) return;
    setPendingYear(year);
    const node = yearRefs.current.get(year);
    if (node) {
      node.scrollIntoView({ behavior: "smooth", block: "start" });
      setActiveYear(year);
      setPendingYear(null);
    }
  }

  function handleJumpMonth(month: TimelineMonth) {
    if (!onJumpToYear(month.year)) return;
    setPendingMonth(month);
    const key = timelineKey(month);
    const node = monthRefs.current.get(key);
    if (node) {
      node.scrollIntoView({ behavior: "smooth", block: "start" });
      setActiveYear(month.year);
      setPendingMonth(null);
    }
  }

  function toggleDay(group: DayGroup) {
    const allSelected = group.assets.every((asset) => selected.has(asset.id));
    for (const asset of group.assets) {
      if (selected.has(asset.id) === allSelected) {
        onToggleSelect(asset.id);
      }
    }
  }

  return (
    <div className="timeline-feed">
      <PageHeader
        className="timeline-page-header"
        title="Timeline"
        description="Your library by capture date. Use the scale to jump to a year or month without endless scrolling."
      />

      {timelineYears.length === 0 && !timelineLoading ? (
        <EmptyState
          icon="calendar"
          title="No dated photos yet"
          description="Photos with a capture date will be arranged here chronologically after import."
          action={{
            label: "Import photos",
            onClick: onImport,
          }}
        />
      ) : (
        <div
          className={
            timelineScaleYears.length > 1 || monthsForActiveYear.length > 0
              ? "timeline-layout has-scale"
              : "timeline-layout"
          }
        >
          <div className="timeline-body">
            {timelineYears.map((year) => (
              <section
                className="timeline-year"
                key={year.year}
                data-year={year.year}
                id={`timeline-year-${year.year}`}
                ref={(node) => {
                  if (node) yearRefs.current.set(year.year, node);
                  else yearRefs.current.delete(year.year);
                }}
              >
                <h2 className="timeline-year-heading">{year.year}</h2>
                <div className="timeline-year-months">
                  {year.months.map((month) => {
                    const key = timelineKey(month);
                    const rows = timelineAssets[key];
                    const dayGroups = rows ? groupAssetsByDay(rows) : [];
                    const monthFullySelected =
                      !!rows &&
                      rows.length > 0 &&
                      rows.every((asset) => selected.has(asset.id));
                    return (
                      <section
                        className="timeline-month"
                        key={key}
                        id={`timeline-month-${key}`}
                        ref={(node) => {
                          if (node) monthRefs.current.set(key, node);
                          else monthRefs.current.delete(key);
                        }}
                      >
                        <header className="timeline-month-heading">
                          <div>
                            <h3>
                              {MONTHS[month.month - 1]} {month.year}
                            </h3>
                            <span className="muted">
                              {month.count}{" "}
                              {month.count === 1 ? "item" : "items"}
                            </span>
                          </div>
                          <button
                            type="button"
                            onClick={() => onSelectMonth(month)}
                            disabled={!rows || rows.length === 0}
                          >
                            {monthFullySelected
                              ? "Unselect month"
                              : "Select month"}
                          </button>
                        </header>
                        {!rows ? (
                          <div
                            className="timeline-month-loading"
                            aria-label={`Loading ${MONTHS[month.month - 1]} ${year.year}`}
                          >
                            {Array.from({
                              length: Math.min(month.count, 5),
                            }).map((_, index) => (
                              <div
                                className="timeline-skeleton"
                                key={index}
                              />
                            ))}
                          </div>
                        ) : (
                          dayGroups.map((group) => {
                            const dayFullySelected =
                              group.assets.length > 0 &&
                              group.assets.every((asset) =>
                                selected.has(asset.id),
                              );
                            return (
                            <section className="timeline-day" key={group.key}>
                              <header className="timeline-day-header">
                                <h4 className="timeline-day-heading">
                                  {group.label}
                                  <span className="muted">
                                    {" "}
                                    · {group.assets.length}
                                  </span>
                                </h4>
                                <button
                                  type="button"
                                  onClick={() => toggleDay(group)}
                                >
                                  {dayFullySelected
                                    ? "Unselect date"
                                    : "Select date"}
                                </button>
                              </header>
                              <div className="asset-grid timeline-asset-grid">
                                {group.assets.map((asset) => (
                                  <AssetCard
                                    key={asset.id}
                                    asset={asset}
                                    isSelected={selected.has(asset.id)}
                                    draggable={selected.has(asset.id)}
                                    trashDays={null}
                                    onDragStart={onAssetDragStart}
                                    onDragEnd={onAssetDragEnd}
                                    onToggleSelect={onToggleSelect}
                                    onToggleFavorite={onToggleFavorite}
                                    onShowInfo={onShowInfo}
                                    onOpen={onOpen}
                                  />
                                ))}
                              </div>
                            </section>
                            );
                          })
                        )}
                      </section>
                    );
                  })}
                </div>
              </section>
            ))}

            <div
              ref={sentinelRef}
              className="timeline-sentinel"
              aria-hidden="true"
            />
            {timelineLoading && (
              <div className="timeline-loading-more" role="status">
                <span className="spinner" aria-hidden="true" />
                Loading more photos…
              </div>
            )}
            {!timelineLoading &&
              timelineVisibleCount >= timelineTotal &&
              timelineTotal > 0 && (
                <p className="timeline-end muted">
                  You’ve reached the beginning of your library.
                </p>
              )}
          </div>

          {(timelineScaleYears.length > 1 || monthsForActiveYear.length > 0) && (
            <TimelineScale
              years={timelineScaleYears}
              months={monthsForActiveYear}
              activeYear={activeYear}
              onJumpYear={handleJumpYear}
              onJumpMonth={handleJumpMonth}
            />
          )}
        </div>
      )}
    </div>
  );
}

function TimelineScale({
  years,
  months,
  activeYear,
  onJumpYear,
  onJumpMonth,
}: {
  years: number[];
  months: TimelineMonth[];
  activeYear: number | null;
  onJumpYear: (year: number) => void;
  onJumpMonth: (month: TimelineMonth) => void;
}) {
  return (
    <nav className="timeline-scale" aria-label="Jump to year or month">
      <div className="timeline-scale-track" aria-hidden="true" />
      <ul className="timeline-scale-years">
        {years.map((year) => {
          const active = year === activeYear;
          return (
            <li className="timeline-scale-entry" key={year}>
              <button
                type="button"
                className={`timeline-scale-year ${active ? "active" : ""}`}
                aria-current={active ? "true" : undefined}
                onClick={() => onJumpYear(year)}
                title={`Jump to ${year}`}
              >
                <span className="timeline-scale-label">{year}</span>
                <span className="timeline-scale-tick" aria-hidden="true" />
              </button>
              {active && months.length > 0 && (
                <ul className="timeline-scale-months">
                  {months.map((month) => (
                    <li key={`${month.year}-${month.month}`}>
                      <button
                        type="button"
                        className="timeline-scale-month"
                        onClick={() => onJumpMonth(month)}
                        title={`Jump to ${MONTHS[month.month - 1]} ${month.year}`}
                      >
                        <span className="timeline-scale-label">
                          {MONTHS[month.month - 1].slice(0, 3)}
                        </span>
                        <span
                          className="timeline-scale-month-tick"
                          aria-hidden="true"
                        />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
