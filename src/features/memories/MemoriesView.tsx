import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type MemoryKind, type MemorySummary } from "../../lib/tauri";

function kindLabel(kind: MemoryKind): string {
  switch (kind) {
    case "onThisDay":
      return "On this day";
    case "weekendTrip":
      return "Weekend trip";
    case "personPlace":
      return "Person & place";
  }
}

function MemoryCover({ memory }: { memory: MemorySummary }) {
  return (
    <div className="memory-cover-media">
      <SafeImage
        src={memory.coverThumbnailPath ? fileSrc(memory.coverThumbnailPath) : null}
        alt=""
        loading="lazy"
        fallback={<MediaFallback type="album" />}
      />
    </div>
  );
}

/** Discover → Memories: curated local stories from dates, people, and places. */
export function MemoriesView({
  memories,
  loading,
  building,
  onOpenMemory,
  onRefresh,
  onDeleteMemory,
}: {
  memories: MemorySummary[];
  /** First read of the cache hasn't returned yet. */
  loading: boolean;
  /** The background builder is grouping memories right now. */
  building: boolean;
  onOpenMemory: (memoryId: string) => void;
  onRefresh: () => void;
  onDeleteMemory: (memoryId: string) => void;
}) {
  const pending = loading || building;
  return (
    <div className="memories-page">
      <PageHeader
        title="Memories"
        description="Stories assembled on this machine from your dates, people, and places — ranked with on-device CLIP when available, with optional offline prose."
        actions={
          <button type="button" onClick={onRefresh} disabled={pending}>
            {building ? "Refreshing…" : "Refresh"}
          </button>
        }
      />
      {pending && memories.length > 0 && (
        <div className="memories-building" role="status">
          <span className="spinner" aria-hidden="true" />
          Looking for new memories…
        </div>
      )}
      {memories.length === 0 ? (
        pending ? (
          <div className="developer-loading" role="status">
            <span className="spinner" aria-hidden="true" />
            Grouping your photos into memories…
          </div>
        ) : (
          <EmptyState
            icon="sparkle"
            title="No memories yet"
            description="Photos from past years on this calendar day, weekend trips, and named people in places will appear here as your library grows."
          />
        )
      ) : (
        <div className="memory-cover-grid">
          {memories.map((memory) => (
            <article key={memory.id} className="memory-cover-card">
              <button
                type="button"
                className="memory-cover-open"
                onClick={() => onOpenMemory(memory.id)}
              >
                <MemoryCover memory={memory} />
                <div className="memory-cover-info">
                  <span className="memory-kind muted">{kindLabel(memory.kind)}</span>
                  <span className="memory-cover-name">{memory.title}</span>
                  <span className={memory.prose || memory.quote ? "memory-prose" : "muted memory-prose"}>
                    {memory.insight}
                  </span>
                </div>
              </button>
              <button
                type="button"
                className="memory-cover-delete"
                title="Remove this memory"
                aria-label={`Remove memory ${memory.title}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onDeleteMemory(memory.id);
                }}
              >
                Remove
              </button>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
