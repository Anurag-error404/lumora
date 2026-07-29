import { useEffect } from "react";
import { Icon, type IconName } from "../../components/icons";
import { SafeImage } from "../../components/SafeImage";
import { MediaFallback } from "../../components/MediaFallback";
import {
  api,
  fileSrc,
  type AssetSummary,
  type LibraryStats,
  type MemorySummary,
  type SmartCounts,
} from "../../lib/tauri";
import type { View } from "../../types/app";

type QuickAction = {
  id: View;
  label: string;
  hint: string;
  icon: IconName;
};

const QUICK_ACTIONS: QuickAction[] = [
  { id: "library", label: "Library", hint: "Browse everything", icon: "library" },
  { id: "timeline", label: "Timeline", hint: "By date", icon: "calendar" },
  { id: "favorites", label: "Favorites", hint: "Your best shots", icon: "star" },
  { id: "memories", label: "Memories", hint: "On this day & trips", icon: "sparkle" },
  { id: "recentViewed", label: "Recently viewed", hint: "Pick up where you left", icon: "eye" },
  { id: "albums", label: "Albums", hint: "Organise collections", icon: "album" },
];

const FEATURES: {
  title: string;
  body: string;
  icon: IconName;
  action: View;
  cta: string;
}[] = [
  {
    title: "Encrypted Locked folder",
    body: "Move private photos and whole albums into a vault that stays ciphertext on disk — even in Finder.",
    icon: "lock",
    action: "locked",
    cta: "Open Locked folder",
  },
  {
    title: "Smart collections",
    body: "Videos, RAW files, screenshots, selfies, and panoramas group themselves automatically so you can jump straight to what you need.",
    icon: "sparkle",
    action: "videos",
    cta: "Browse videos",
  },
  {
    title: "Duplicates & blur cleanup",
    body: "Exact matches clean in bulk; near-duplicates and blurry shots stay review-only so you decide what leaves the library.",
    icon: "copy",
    action: "duplicates",
    cta: "Review duplicates",
  },
  {
    title: "Watched folders",
    body: "Point LUMORA at a folder and new photos appear in your library as they land on disk.",
    icon: "folder",
    action: "watched",
    cta: "Manage watched folders",
  },
];

/**
 * App home: brand presence, quick jumps into the most-used destinations,
 * and a short feature highlight strip.
 */
export function HomeView({
  stats,
  smartCounts,
  recent,
  memories,
  onRecentLoaded,
  onNavigate,
  onImport,
  onOpenAsset,
  onOpenMemory,
}: {
  stats: LibraryStats | null;
  smartCounts: SmartCounts | null;
  recent: AssetSummary[];
  memories: MemorySummary[];
  onRecentLoaded: (rows: AssetSummary[]) => void;
  onNavigate: (view: View) => void;
  onImport: () => void;
  onOpenAsset: (id: string) => void;
  onOpenMemory: (memoryId: string) => void;
}) {
  useEffect(() => {
    let cancelled = false;
    void api
      .listRecent(8, 0)
      .then((rows) => {
        if (!cancelled) onRecentLoaded(rows);
      })
      .catch(() => {
        if (!cancelled) onRecentLoaded([]);
      });
    return () => {
      cancelled = true;
    };
  }, [stats?.totalAssets, onRecentLoaded]);

  const empty = !stats || stats.totalAssets === 0;

  return (
    <div className="home">
      <section className="home-hero">
        <div className="home-hero-copy">
          <p className="home-kicker">Local photo library</p>
          <h1 className="home-brand">
            <img
              className="home-brand-icon"
              src="/lumora-icon-tp.png"
              width={40}
              height={40}
              alt=""
            />
            LUMORA
          </h1>
          <p className="home-tagline">your memories your machine.</p>
          <div className="home-hero-actions">
            <button className="primary" onClick={onImport}>
              + Import photos
            </button>
            <button className="secondary" onClick={() => onNavigate("library")}>
              Open library
            </button>
          </div>
          {stats && (
            <p className="home-hero-stats muted">
              {stats.totalAssets} assets · {stats.totalImages} photos ·{" "}
              {stats.totalVideos} videos
              {smartCounts
                ? ` · ${smartCounts.screenshots} screenshots · ${smartCounts.selfies} selfies · ${smartCounts.rawPhotos} RAW`
                : ""}
            </p>
          )}
        </div>
        <div className="home-hero-glow" aria-hidden="true" />
      </section>

      <section className="home-section">
        <header className="home-section-head">
          <h2>Quick access</h2>
          <p className="muted">Jump into the places you use most.</p>
        </header>
        <div className="home-quick-grid">
          {QUICK_ACTIONS.map((item) => (
            <button
              key={item.id}
              type="button"
              className="home-quick"
              onClick={() => onNavigate(item.id)}
            >
              <Icon name={item.icon} className="home-quick-icon" />
              <span className="home-quick-label">{item.label}</span>
              <span className="home-quick-hint">{item.hint}</span>
            </button>
          ))}
        </div>
      </section>

      {!empty && memories.length > 0 && (
        <section className="home-section">
          <header className="home-section-head">
            <div>
              <h2>Memories</h2>
              <p className="muted">On this day, weekend trips, and people in places.</p>
            </div>
            <button className="secondary" onClick={() => onNavigate("memories")}>
              See all
            </button>
          </header>
          <div className="home-memory-strip">
            {memories.slice(0, 6).map((memory) => {
              const src = memory.coverThumbnailPath
                ? fileSrc(memory.coverThumbnailPath)
                : null;
              return (
                <button
                  key={memory.id}
                  type="button"
                  className="home-memory-card"
                  onClick={() => onOpenMemory(memory.id)}
                  title={memory.subtitle}
                >
                  <div className="home-memory-cover">
                    <SafeImage
                      src={src}
                      alt=""
                      loading="lazy"
                      fallback={<MediaFallback type="album" compact />}
                    />
                  </div>
                  <span className="home-memory-title">{memory.title}</span>
                  <span className="home-memory-sub muted">{memory.subtitle}</span>
                </button>
              );
            })}
          </div>
        </section>
      )}

      {!empty && recent.length > 0 && (
        <section className="home-section">
          <header className="home-section-head">
            <div>
              <h2>Recently added</h2>
              <p className="muted">Your latest imports, ready to review.</p>
            </div>
            <button className="secondary" onClick={() => onNavigate("recent")}>
              See all
            </button>
          </header>
          <div className="home-recent-strip">
            {recent.map((asset) => {
              const src = asset.thumbnailPath ? fileSrc(asset.thumbnailPath) : null;
              return (
                <button
                  key={asset.id}
                  type="button"
                  className="home-recent-tile"
                  onClick={() => onOpenAsset(asset.id)}
                  title={asset.path}
                >
                  <SafeImage
                    src={src}
                    alt=""
                    loading="lazy"
                    fallback={
                      <MediaFallback
                        type={asset.mediaType === "video" ? "video" : "image"}
                        compact
                      />
                    }
                  />
                </button>
              );
            })}
          </div>
        </section>
      )}

      <section className="home-section">
        <header className="home-section-head">
          <h2>Built for local privacy</h2>
          <p className="muted">The features that make LUMORA yours alone.</p>
        </header>
        <div className="home-features">
          {FEATURES.map((feature) => (
            <article key={feature.title} className="home-feature">
              <Icon name={feature.icon} className="home-feature-icon" />
              <div className="home-feature-copy">
                <h3>{feature.title}</h3>
                <p className="muted">{feature.body}</p>
              </div>
              <button className="secondary" onClick={() => onNavigate(feature.action)}>
                {feature.cta}
              </button>
            </article>
          ))}
        </div>
      </section>

      {empty && (
        <section className="home-empty-call">
          <h2>Start with a folder</h2>
          <p className="muted">
            Import photos or watch a directory. Everything stays on this machine —
            no cloud account, no upload.
          </p>
          <button className="primary" onClick={onImport}>
            Import your first photos
          </button>
        </section>
      )}
    </div>
  );
}
