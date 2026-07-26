import {
  useEffect,
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

/** Chronological feed grouped by year and month with a year jump scale. */
export function TimelineView({
  timelineYears,
  timelineScaleYears,
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
  const yearRefs = useRef(new Map<number, HTMLElement>());

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

  // After jump expands the lazy window, scroll to the year heading.
  useEffect(() => {
    if (pendingYear == null) return;
    const node = yearRefs.current.get(pendingYear);
    if (!node) return;
    node.scrollIntoView({ behavior: "smooth", block: "start" });
    setActiveYear(pendingYear);
    setPendingYear(null);
  }, [pendingYear, timelineYears]);

  function handleJump(year: number) {
    if (!onJumpToYear(year)) return;
    setPendingYear(year);
    // Already mounted — scroll immediately.
    const node = yearRefs.current.get(year);
    if (node) {
      node.scrollIntoView({ behavior: "smooth", block: "start" });
      setActiveYear(year);
      setPendingYear(null);
    }
  }

  return (
    <div className="timeline-feed">
      <PageHeader
        className="timeline-page-header"
        title="Timeline"
        description="Your library by capture date. Use the year scale to jump across long histories without endless scrolling."
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
            timelineScaleYears.length > 1
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
                    const monthFullySelected =
                      !!rows &&
                      rows.length > 0 &&
                      rows.every((asset) => selected.has(asset.id));
                    return (
                      <section className="timeline-month" key={key}>
                        <header className="timeline-month-heading">
                          <div>
                            <h3>{MONTHS[month.month - 1]}</h3>
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
                          <div className="asset-grid timeline-asset-grid">
                            {rows.map((asset) => (
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

          {timelineScaleYears.length > 1 && (
            <TimelineScale
              years={timelineScaleYears}
              activeYear={activeYear}
              onJump={handleJump}
            />
          )}
        </div>
      )}
    </div>
  );
}

function TimelineScale({
  years,
  activeYear,
  onJump,
}: {
  years: number[];
  activeYear: number | null;
  onJump: (year: number) => void;
}) {
  return (
    <nav className="timeline-scale" aria-label="Jump to year">
      <div className="timeline-scale-track" aria-hidden="true" />
      <ul className="timeline-scale-years">
        {years.map((year) => {
          const active = year === activeYear;
          return (
            <li key={year}>
              <button
                type="button"
                className={`timeline-scale-year ${active ? "active" : ""}`}
                aria-current={active ? "true" : undefined}
                onClick={() => onJump(year)}
                title={`Jump to ${year}`}
              >
                <span className="timeline-scale-label">{year}</span>
                <span className="timeline-scale-tick" aria-hidden="true" />
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
