import { PageHeader } from "../../components/PageHeader";
import { LABEL_COLORS } from "../../lib/constants";
import { labelName } from "../../lib/labels";
import type { Tag, TagBrowseFilter } from "../../lib/tauri";

/** The Tags view: combinable tag / rating / colour filter chips. */
export function TagFilterBoard({
  tags,
  tagBrowse,
  tagBrowseActive,
  ratingCounts,
  colorCounts,
  tagBrowseSummary,
  onToggleTag,
  onToggleRating,
  onToggleColor,
  onClearAll,
}: {
  tags: Tag[];
  tagBrowse: TagBrowseFilter;
  tagBrowseActive: boolean;
  ratingCounts: Map<number, number>;
  colorCounts: Map<string, number>;
  tagBrowseSummary: string[];
  onToggleTag: (tagId: string) => void;
  onToggleRating: (rating: number) => void;
  onToggleColor: (color: string) => void;
  onClearAll: () => void;
}) {
  return (
    <div className="tags-page">
      <PageHeader
        title="Tags & labels"
        description="Combine tags, ratings, and colour labels to narrow the library. Different groups narrow together; the same group broadens."
        actions={
          tagBrowseActive ? (
            <button type="button" onClick={onClearAll}>
              Clear all
            </button>
          ) : undefined
        }
      />

      <div className="tag-filter-board" role="group" aria-label="Filters">
        <section className="tag-filter-col" aria-labelledby="tag-filter-tags">
          <div className="tag-filter-col-head">
            <h3 id="tag-filter-tags">Tags</h3>
            {tagBrowse.tagIds.length > 0 && (
              <span className="tag-filter-badge">
                {tagBrowse.tagIds.length}
              </span>
            )}
          </div>
          <div className="tag-filter-chips" role="list">
            {tags.length === 0 ? (
              <p className="tag-filter-empty muted">
                No tags yet. Select photos and choose Tag selection.
              </p>
            ) : (
              tags.map((tag) => {
                const on = tagBrowse.tagIds.includes(tag.id);
                return (
                  <button
                    key={tag.id}
                    type="button"
                    role="listitem"
                    aria-pressed={on}
                    className={`tag-filter-chip ${on ? "on" : ""}`}
                    onClick={() => onToggleTag(tag.id)}
                  >
                    <span className="tag-filter-chip-label">
                      {tag.name}
                    </span>
                    <span className="tag-filter-chip-count">
                      {tag.assetCount}
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </section>

        <section
          className="tag-filter-col"
          aria-labelledby="tag-filter-ratings"
        >
          <div className="tag-filter-col-head">
            <h3 id="tag-filter-ratings">Ratings</h3>
            {tagBrowse.ratings.length > 0 && (
              <span className="tag-filter-badge">
                {tagBrowse.ratings.length}
              </span>
            )}
          </div>
          <div className="tag-filter-chips" role="list">
            {[5, 4, 3, 2, 1].map((rating) => {
              const count = ratingCounts.get(rating) ?? 0;
              const on = tagBrowse.ratings.includes(rating);
              return (
                <button
                  key={rating}
                  type="button"
                  role="listitem"
                  aria-pressed={on}
                  disabled={count === 0 && !on}
                  className={`tag-filter-chip tag-filter-rating ${on ? "on" : ""}`}
                  onClick={() => onToggleRating(rating)}
                  title={`${rating} star${rating === 1 ? "" : "s"}`}
                >
                  <span
                    className="tag-facet-stars"
                    aria-hidden="true"
                  >
                    {"★".repeat(rating)}
                    <span className="tag-facet-stars-empty">
                      {"★".repeat(5 - rating)}
                    </span>
                  </span>
                  <span className="tag-filter-chip-count">{count}</span>
                </button>
              );
            })}
          </div>
        </section>

        <section
          className="tag-filter-col"
          aria-labelledby="tag-filter-colours"
        >
          <div className="tag-filter-col-head">
            <h3 id="tag-filter-colours">Colours</h3>
            {tagBrowse.colorLabels.length > 0 && (
              <span className="tag-filter-badge">
                {tagBrowse.colorLabels.length}
              </span>
            )}
          </div>
          <div className="tag-filter-chips" role="list">
            {LABEL_COLORS.map((color) => {
              const count = colorCounts.get(color.id) ?? 0;
              const on = tagBrowse.colorLabels.includes(color.id);
              return (
                <button
                  key={color.id}
                  type="button"
                  role="listitem"
                  aria-pressed={on}
                  disabled={count === 0 && !on}
                  className={`tag-filter-chip ${on ? "on" : ""}`}
                  onClick={() => onToggleColor(color.id)}
                >
                  <span
                    className="tag-facet-dot"
                    style={{ background: color.hex }}
                    aria-hidden="true"
                  />
                  <span className="tag-filter-chip-label">
                    {labelName(color.id)}
                  </span>
                  <span className="tag-filter-chip-count">{count}</span>
                </button>
              );
            })}
          </div>
        </section>
      </div>

      {tagBrowseActive && (
        <div className="tag-filter-active" aria-live="polite">
          <span className="tag-filter-active-label">Matching</span>
          <div className="tag-filter-active-list">
            {tagBrowseSummary.map((part, i) => (
              <span key={`${part}-${i}`} className="tag-filter-active-chip">
                {part}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
