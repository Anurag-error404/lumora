import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { PageHeader } from "../../components/PageHeader";
import { formatBytes } from "../../lib/format";
import { MAKER } from "../../lib/maker";
import {
  api,
  type Preferences,
  type StorageSummary,
  type VaultStatus,
} from "../../lib/tauri";
import { AiModelsPanel } from "./AiModelsPanel";
import { ModelLibraryPanel } from "./ModelLibraryPanel";
import {
  ChoiceRow,
  SelectRow,
  SettingsBlock,
  SETTINGS_NAV,
  SliderRow,
  ToggleRow,
  type PrefsUpdater,
  type SettingsSectionId,
} from "./settingsUi";
import type { useAppUpdater } from "../../hooks/useAppUpdater";

const SHORTCUTS: { action: string; keys: string }[] = [
  { action: "Next photo", keys: "→" },
  { action: "Previous photo", keys: "←" },
  { action: "Favorite", keys: "F" },
  { action: "Rate 0–5", keys: "0–5" },
  { action: "Delete / Restore", keys: "Delete" },
  { action: "Open / close viewer", keys: "Space" },
  { action: "Select all", keys: "⌘A" },
  { action: "Undo", keys: "⌘Z" },
  { action: "Redo", keys: "⌘⇧Z" },
  { action: "Close panels", keys: "Esc" },
];

/** Multi-page Settings shell — user preferences only; diagnostics stay in Developer. */
export function SettingsView({
  prefs,
  loading,
  update,
  watchedFolders,
  watchedLoading,
  busy,
  onAddFolder,
  onRemoveFolder,
  onRefreshWatched,
  onOpenPath,
  onOpenLocked,
  vaultStatus,
  appVersion,
  updater,
}: {
  prefs: Preferences | null;
  loading: boolean;
  update: PrefsUpdater;
  watchedFolders: string[];
  watchedLoading: boolean;
  busy: boolean;
  onAddFolder: () => void;
  onRemoveFolder: (path: string) => void;
  onRefreshWatched: () => void;
  onOpenPath: (path: string, reveal?: boolean) => void;
  onOpenLocked: () => void;
  vaultStatus: VaultStatus | null;
  appVersion: string;
  updater: ReturnType<typeof useAppUpdater>;
}) {
  const [section, setSection] = useState<SettingsSectionId>("general");
  const [storage, setStorage] = useState<StorageSummary | null>(null);
  const [storageBusy, setStorageBusy] = useState(false);
  const [storageMsg, setStorageMsg] = useState<string | null>(null);

  const refreshStorage = useCallback(async () => {
    try {
      setStorage(await api.getStorageSummary());
    } catch (e) {
      setStorageMsg(String(e));
    }
  }, []);

  useEffect(() => {
    if (section === "storage") void refreshStorage();
  }, [section, refreshStorage]);

  const navLabel = SETTINGS_NAV.find((n) => n.id === section)?.label ?? "Settings";

  return (
    <div className="settings-page">
      <PageHeader
        title="Settings"
        description="Preferences that shape how LUMORA looks and behaves. Technical diagnostics live under Developer."
      />

      <div className="settings-shell">
        <nav className="settings-nav" aria-label="Settings sections">
          {SETTINGS_NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className={section === item.id ? "is-active" : ""}
              onClick={() => setSection(item.id)}
            >
              {item.label}
            </button>
          ))}
        </nav>

        <div className="settings-content" aria-live="polite">
          <h2 className="settings-section-title">{navLabel}</h2>

          {loading || !prefs ? (
            <div className="developer-loading" role="status">
              <span className="spinner" aria-hidden="true" />
              Loading preferences…
            </div>
          ) : section === "general" ? (
            <GeneralSection prefs={prefs} update={update} />
          ) : section === "appearance" ? (
            <AppearanceSection prefs={prefs} update={update} />
          ) : section === "library" ? (
            <LibrarySection
              prefs={prefs}
              update={update}
              folders={watchedFolders}
              loading={watchedLoading}
              busy={busy}
              onAdd={onAddFolder}
              onRemove={onRemoveFolder}
              onRefresh={onRefreshWatched}
            />
          ) : section === "ai" ? (
            <AiSection prefs={prefs} update={update} />
          ) : section === "privacy" ? (
            <PrivacySection
              prefs={prefs}
              update={update}
              vaultStatus={vaultStatus}
              onOpenLocked={onOpenLocked}
            />
          ) : section === "storage" ? (
            <StorageSection
              storage={storage}
              busy={storageBusy}
              message={storageMsg}
              onRefresh={() => void refreshStorage()}
              onOpenPath={onOpenPath}
              onClearCache={async () => {
                if (
                  !window.confirm(
                    "Clear thumbnail cache?\n\nPreviews will regenerate as you browse. Your photos are not affected.",
                  )
                )
                  return;
                setStorageBusy(true);
                setStorageMsg(null);
                try {
                  const n = await api.clearThumbnailCache();
                  setStorageMsg(`Cleared ${n} cached preview(s).`);
                  await refreshStorage();
                } catch (e) {
                  setStorageMsg(String(e));
                } finally {
                  setStorageBusy(false);
                }
              }}
              onRebuild={async () => {
                if (
                  !window.confirm(
                    "Rebuild thumbnail cache?\n\nThis clears previews and regenerates them for your library. It may take a while on large collections.",
                  )
                )
                  return;
                setStorageBusy(true);
                setStorageMsg(null);
                try {
                  const n = await api.rebuildThumbnailCache();
                  setStorageMsg(`Rebuilt ${n} thumbnail(s).`);
                  await refreshStorage();
                } catch (e) {
                  setStorageMsg(String(e));
                } finally {
                  setStorageBusy(false);
                }
              }}
              onOptimize={async () => {
                setStorageBusy(true);
                setStorageMsg(null);
                try {
                  await api.optimizeDatabase();
                  setStorageMsg("Database optimized.");
                  await refreshStorage();
                } catch (e) {
                  setStorageMsg(String(e));
                } finally {
                  setStorageBusy(false);
                }
              }}
            />
          ) : section === "performance" ? (
            <PerformanceSection prefs={prefs} update={update} />
          ) : section === "shortcuts" ? (
            <ShortcutsSection />
          ) : section === "importExport" ? (
            <ImportExportSection prefs={prefs} update={update} />
          ) : section === "updates" ? (
            <UpdatesSection
              appVersion={appVersion}
              prefs={prefs}
              update={update}
              updater={updater}
            />
          ) : section === "about" ? (
            <AboutSection appVersion={appVersion} />
          ) : (
            <FromMakerSection />
          )}
        </div>
      </div>
    </div>
  );
}

function GeneralSection({
  prefs,
  update,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
}) {
  const g = prefs.general;
  return (
    <>
      <SettingsBlock title="Startup">
        <ToggleRow
          label="Restore previous session"
          description="Remember the last view when the app opens (applied on next launch)."
          checked={g.restorePreviousSession}
          onChange={(v) =>
            void update((p) => {
              p.general.restorePreviousSession = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Behavior">
        <ToggleRow
          label="Double click opens viewer"
          description="When on, a single click selects; double-click opens the viewer."
          checked={g.doubleClickOpensViewer}
          onChange={(v) =>
            void update((p) => {
              p.general.doubleClickOpensViewer = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Confirm before deleting"
          description="Ask before moving photos to Trash."
          checked={g.confirmBeforeDeleting}
          onChange={(v) =>
            void update((p) => {
              p.general.confirmBeforeDeleting = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Automatically reveal imported photos"
          checked={g.revealImportedPhotos}
          onChange={(v) =>
            void update((p) => {
              p.general.revealImportedPhotos = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Language & region">
        <SelectRow
          label="Language"
          value={g.language}
          options={[{ value: "en", label: "English" }]}
          onChange={(v) =>
            void update((p) => {
              p.general.language = v;
              return p;
            })
          }
        />
        <SelectRow
          label="Date format"
          value={g.dateFormat}
          options={[
            { value: "dd/mm/yyyy", label: "DD/MM/YYYY" },
            { value: "mm/dd/yyyy", label: "MM/DD/YYYY" },
            { value: "yyyy-mm-dd", label: "YYYY-MM-DD" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.general.dateFormat = v;
              return p;
            })
          }
        />
      </SettingsBlock>
    </>
  );
}

function AppearanceSection({
  prefs,
  update,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
}) {
  const a = prefs.appearance;
  return (
    <>
      <SettingsBlock title="Grid">
        <SliderRow
          label="Thumbnail size"
          description="Small → Large"
          value={a.thumbnailSize}
          min={100}
          max={280}
          step={10}
          format={(n) => `${n}px`}
          onChange={(v) =>
            void update((p) => {
              p.appearance.thumbnailSize = v;
              return p;
            })
          }
        />
        <ChoiceRow
          label="Density"
          value={a.density}
          options={[
            { value: "comfortable", label: "Comfortable" },
            { value: "compact", label: "Compact" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.appearance.density = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Motion">
        <ToggleRow
          label="Enable animations"
          checked={a.animations}
          onChange={(v) =>
            void update((p) => {
              p.appearance.animations = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Smooth scrolling"
          checked={a.smoothScrolling}
          onChange={(v) =>
            void update((p) => {
              p.appearance.smoothScrolling = v;
              return p;
            })
          }
        />
      </SettingsBlock>
    </>
  );
}

function LibrarySection({
  prefs,
  update,
  folders,
  loading,
  busy,
  onAdd,
  onRemove,
  onRefresh,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
  folders: string[];
  loading: boolean;
  busy: boolean;
  onAdd: () => void;
  onRemove: (path: string) => void;
  onRefresh: () => void;
}) {
  return (
    <>
      <SettingsBlock title="Library locations">
        <div className="settings-folder-list">
          {loading && folders.length === 0 ? (
            <p className="muted">Loading folders…</p>
          ) : folders.length === 0 ? (
            <p className="muted">
              No watched folders yet. Add a folder to keep the library in sync.
            </p>
          ) : (
            <ul>
              {folders.map((path) => {
                const name = path.split(/[/\\]/).filter(Boolean).pop() ?? path;
                return (
                  <li key={path}>
                    <div>
                      <strong>{name}</strong>
                      <span className="muted developer-path">{path}</span>
                    </div>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onRemove(path)}
                    >
                      Remove
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          <div className="settings-inline-actions">
            <button className="primary" type="button" disabled={busy} onClick={onAdd}>
              + Add Folder
            </button>
            <button type="button" disabled={busy || loading} onClick={onRefresh}>
              Refresh
            </button>
          </div>
        </div>
      </SettingsBlock>
      <SettingsBlock title="Watching">
        <ToggleRow
          label="Automatically watch folders"
          description="Index new and changed files as they appear on disk."
          checked={prefs.library.watchFoldersEnabled}
          onChange={(v) =>
            void update((p) => {
              p.library.watchFoldersEnabled = v;
              return p;
            })
          }
        />
        <SelectRow
          label="Auto-scan watched folders"
          description="Re-index watched folders on launch, hourly, or daily."
          value={prefs.library.autoScan}
          options={[
            { value: "manual", label: "Manual" },
            { value: "on_launch", label: "On launch" },
            { value: "hourly", label: "Hourly" },
            { value: "daily", label: "Daily" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.library.autoScan = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Ignored paths">
        <label className="settings-row">
          <span className="settings-row-copy">
            <span className="settings-row-label">Ignore patterns</span>
            <span className="muted">One pattern per line, such as *.tmp or /cache/.</span>
          </span>
          <textarea
            rows={5}
            value={prefs.library.ignorePatterns.join("\n")}
            onChange={(event) =>
              void update((p) => {
                p.library.ignorePatterns = event.target.value
                  .split("\n")
                  .map((pattern) => pattern.trim())
                  .filter(Boolean);
                return p;
              })
            }
          />
        </label>
      </SettingsBlock>
    </>
  );
}

function AiSection({
  prefs,
  update,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
}) {
  return (
    <>
      <SettingsBlock title="AI processing">
        <ToggleRow
          label="Semantic Search"
          description="Natural-language search with on-device CLIP embeddings."
          checked={prefs.ai.semanticSearch}
          onChange={(v) =>
            void update((p) => {
              p.ai.semanticSearch = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Text recognition (OCR)"
          description="Extract text from photos on-device for search and smart collections."
          checked={prefs.ai.ocr}
          onChange={(v) =>
            void update((p) => {
              p.ai.ocr = v;
              return p;
            }).then(() => {
              if (v) void api.kickOcr();
            })
          }
        />
        <ToggleRow
          label="Face recognition"
          description="Detect and group faces on-device for the People view."
          checked={prefs.ai.faceRecognition}
          onChange={(v) =>
            void update((p) => {
              p.ai.faceRecognition = v;
              return p;
            }).then(() => {
              if (v) void api.kickFaces();
            })
          }
        />
        <ToggleRow
          label="Object detection / auto-tags"
          description="Classify photos with on-device MobileNetV4 labels for search."
          checked={prefs.ai.objectDetection}
          onChange={(v) =>
            void update((p) => {
              p.ai.objectDetection = v;
              return p;
            }).then(() => {
              if (v) void api.kickTags();
            })
          }
        />
        <ToggleRow
          label="Image captions"
          description="Generate private, on-device descriptions with Florence-2 for search."
          checked={prefs.ai.captions}
          onChange={(v) =>
            void update((p) => {
              p.ai.captions = v;
              return p;
            }).then(() => {
              if (v) void api.kickCaptions();
            })
          }
        />
        <ChoiceRow
          label="Background processing"
          description="Choose whether on-device AI and Places jobs run continuously or only after inactivity."
          value={prefs.ai.backgroundProcessing}
          options={[
            { value: "always", label: "Always" },
            { value: "idle", label: "When idle" },
            { value: "paused", label: "Paused" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.ai.backgroundProcessing = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Create place albums automatically"
          description="Add geotagged photos to a named album after offline reverse geocoding."
          checked={prefs.ai.autoAlbums}
          onChange={(v) =>
            void update((p) => {
              p.ai.autoAlbums = v;
              return p;
            })
          }
        />
        <ChoiceRow
          label="Processing device"
          description="Controls the worker's runtime preference."
          value={prefs.ai.processingDevice}
          options={[
            { value: "automatic", label: "Automatic" },
            { value: "cpu", label: "CPU" },
            { value: "gpu", label: "GPU" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.ai.processingDevice = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Models">
        <AiModelsPanel updatePrefs={update} />
      </SettingsBlock>
      <SettingsBlock title="Model library">
        <ModelLibraryPanel />
      </SettingsBlock>
    </>
  );
}

function PrivacySection({
  prefs,
  update,
  vaultStatus,
  onOpenLocked,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
  vaultStatus: VaultStatus | null;
  onOpenLocked: () => void;
}) {
  const p = prefs.privacy;
  return (
    <>
      <SettingsBlock title="Locked folder">
        <div className="settings-vault-card">
          <div>
            <strong>
              {vaultStatus?.configured
                ? vaultStatus.unlocked
                  ? "Unlocked"
                  : "Locked"
                : "Not set up"}
            </strong>
            <p className="muted">
              Encrypted vault for private albums and photos. Keys never leave this
              machine.
            </p>
          </div>
          <button type="button" className="primary" onClick={onOpenLocked}>
            Open Locked folder
          </button>
        </div>
        <SelectRow
          label="Auto-lock vault"
          description="Lock after inactivity while a vault is unlocked. Never keeps it open until you lock manually."
          value={String(p.autoLockMinutes)}
          options={[
            { value: "0", label: "Never" },
            { value: "5", label: "5 minutes" },
            { value: "15", label: "15 minutes" },
            { value: "30", label: "30 minutes" },
            { value: "60", label: "1 hour" },
          ]}
          onChange={(v) =>
            void update((cur) => {
              cur.privacy.autoLockMinutes = Number(v);
              return cur;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Metadata">
        <ToggleRow
          label="Preserve GPS location data"
          description="When off, location extraction skips GPS data and does not store Places records."
          checked={p.preserveGps}
          onChange={(v) =>
            void update((cur) => {
              cur.privacy.preserveGps = v;
              return cur;
            })
          }
        />
        <ToggleRow
          label="Preserve EXIF metadata"
          description="Read camera, lens, and capture-date metadata when importing."
          checked={p.preserveExif}
          onChange={(v) =>
            void update((cur) => {
              cur.privacy.preserveExif = v;
              return cur;
            })
          }
        />
        <ToggleRow
          label="Strip metadata on export"
          description="Re-encode still images without EXIF/GPS when exporting a ZIP. Videos are copied as-is."
          checked={p.stripMetadataOnExport}
          onChange={(v) =>
            void update((cur) => {
              cur.privacy.stripMetadataOnExport = v;
              return cur;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Crash reports">
        <p className="muted settings-note">
          LUMORA never sends crash reports or analytics. Diagnostics stay in local
          log files under Developer.
        </p>
      </SettingsBlock>
    </>
  );
}

function PerformanceSection({
  prefs,
  update,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
}) {
  const performance = prefs.performance;
  return (
    <SettingsBlock title="Background work">
      <ChoiceRow
        label="CPU profile"
        description="Balances worker throughput against foreground responsiveness."
        value={performance.cpuProfile}
        options={[
          { value: "eco", label: "Eco" },
          { value: "balanced", label: "Balanced" },
          { value: "aggressive", label: "Aggressive" },
        ]}
        onChange={(v) =>
          void update((p) => {
            p.performance.cpuProfile = v;
            return p;
          })
        }
      />
      <ToggleRow
        label="Pause background work on battery"
        checked={performance.pauseOnBattery}
        onChange={(v) =>
          void update((p) => {
            p.performance.pauseOnBattery = v;
            return p;
          })
        }
      />
      <SliderRow
        label="Thumbnail cache budget"
        description="Used while generating new thumbnail previews."
        value={performance.thumbnailCacheMb}
        min={128}
        max={4096}
        step={128}
        format={(n) => `${n} MB`}
        onChange={(v) =>
          void update((p) => {
            p.performance.thumbnailCacheMb = v;
            return p;
          })
        }
      />
    </SettingsBlock>
  );
}

function StorageSection({
  storage,
  busy,
  message,
  onRefresh,
  onOpenPath,
  onClearCache,
  onRebuild,
  onOptimize,
}: {
  storage: StorageSummary | null;
  busy: boolean;
  message: string | null;
  onRefresh: () => void;
  onOpenPath: (path: string, reveal?: boolean) => void;
  onClearCache: () => Promise<void>;
  onRebuild: () => Promise<void>;
  onOptimize: () => Promise<void>;
}) {
  return (
    <>
      <SettingsBlock title="Storage">
        {!storage ? (
          <div className="developer-loading" role="status">
            <span className="spinner" aria-hidden="true" />
            Measuring storage…
          </div>
        ) : (
          <dl className="settings-storage-list">
            <div>
              <dt>Library database</dt>
              <dd>{formatBytes(storage.databaseBytes)}</dd>
            </div>
            <div>
              <dt>Thumbnail cache</dt>
              <dd>
                {formatBytes(storage.thumbnailBytes)}
                <span className="muted"> · {storage.thumbnailCount} files</span>
              </dd>
            </div>
            <div>
              <dt>AI models</dt>
              <dd>{formatBytes(storage.modelsBytes)}</dd>
            </div>
            <div>
              <dt>AI embeddings</dt>
              <dd>{formatBytes(storage.embeddingsBytes)}</dd>
            </div>
            <div>
              <dt>Logs</dt>
              <dd>{formatBytes(storage.logsBytes)}</dd>
            </div>
          </dl>
        )}
        {message && <p className="muted">{message}</p>}
        <div className="settings-inline-actions">
          <button type="button" disabled={busy} onClick={() => void onClearCache()}>
            Clear Thumbnail Cache
          </button>
          <button type="button" disabled={busy} onClick={() => void onRebuild()}>
            Rebuild Cache
          </button>
          <button type="button" disabled={busy} onClick={() => void onOptimize()}>
            Optimize Database
          </button>
          <button type="button" disabled={busy} onClick={onRefresh}>
            Refresh
          </button>
        </div>
      </SettingsBlock>
      {storage && (
        <SettingsBlock title="Folders">
          <div className="settings-path-actions">
            <button type="button" onClick={() => onOpenPath(storage.appDataPath)}>
              Open Data Folder
            </button>
            <button type="button" onClick={() => onOpenPath(storage.thumbsPath)}>
              Open Cache Folder
            </button>
            <button type="button" onClick={() => onOpenPath(storage.modelsPath)}>
              Open Models Folder
            </button>
            <button
              type="button"
              onClick={() => onOpenPath(storage.databasePath, true)}
            >
              Show Database
            </button>
          </div>
        </SettingsBlock>
      )}
    </>
  );
}

function ShortcutsSection() {
  return (
    <SettingsBlock title="Keyboard shortcuts">
      <ul className="settings-shortcuts">
        {SHORTCUTS.map((row) => (
          <li key={row.action}>
            <span>{row.action}</span>
            <kbd>{row.keys}</kbd>
          </li>
        ))}
      </ul>
    </SettingsBlock>
  );
}

function ImportExportSection({
  prefs,
  update,
}: {
  prefs: Preferences;
  update: PrefsUpdater;
}) {
  const ie = prefs.importExport;
  return (
    <>
      <SettingsBlock title="Import">
        <ToggleRow
          label="Skip duplicates"
          description="Skip files whose content hash already exists in the library (different path, same bytes)."
          checked={ie.skipDuplicates}
          onChange={(v) =>
            void update((p) => {
              p.importExport.skipDuplicates = v;
              return p;
            })
          }
        />
      </SettingsBlock>
      <SettingsBlock title="Export">
        <ToggleRow
          label="Preserve folder structure"
          description="Keep relative folders inside exported ZIP files."
          checked={ie.preserveFolderStructure}
          onChange={(v) =>
            void update((p) => {
              p.importExport.preserveFolderStructure = v;
              return p;
            })
          }
        />
        <SliderRow
          label="JPEG quality"
          description="Used when stripping metadata re-encodes JPEGs on export."
          value={ie.jpegQuality}
          min={60}
          max={100}
          format={(n) => `${n}%`}
          onChange={(v) =>
            void update((p) => {
              p.importExport.jpegQuality = v;
              return p;
            })
          }
        />
        <SliderRow
          label="Maximum image edge"
          description="Resize exported still images; 0 keeps original dimensions."
          value={ie.exportMaxEdge}
          min={0}
          max={8192}
          step={256}
          format={(n) => (n === 0 ? "Original" : `${n}px`)}
          onChange={(v) =>
            void update((p) => {
              p.importExport.exportMaxEdge = v;
              return p;
            })
          }
        />
        <SelectRow
          label="File naming"
          value={ie.exportNaming}
          options={[
            { value: "original", label: "Original filename" },
            { value: "date_filename", label: "Date + filename" },
            { value: "sequential", label: "Sequential" },
          ]}
          onChange={(v) =>
            void update((p) => {
              p.importExport.exportNaming = v;
              return p;
            })
          }
        />
        <ToggleRow
          label="Strip metadata"
          description="Re-encode still images without EXIF/GPS. Videos are copied as-is."
          checked={ie.stripMetadata}
          onChange={(v) =>
            void update((p) => {
              p.importExport.stripMetadata = v;
              return p;
            })
          }
        />
      </SettingsBlock>
    </>
  );
}

function UpdatesSection({
  appVersion,
  prefs,
  update,
  updater,
}: {
  appVersion: string;
  prefs: Preferences | null;
  update: PrefsUpdater;
  updater: ReturnType<typeof useAppUpdater>;
}) {
  const u = prefs?.updates;
  const busy =
    updater.status === "checking" ||
    updater.status === "downloading" ||
    updater.status === "ready";
  const pct =
    updater.progress?.contentLength && updater.progress.contentLength > 0
      ? Math.min(
          100,
          Math.round(
            (updater.progress.downloaded / updater.progress.contentLength) * 100,
          ),
        )
      : null;

  let statusLabel = "Not checked yet";
  if (updater.isDev) {
    statusLabel = "Updater runs in release builds only";
  } else if (updater.status === "checking") {
    statusLabel = "Checking for updates…";
  } else if (updater.status === "upToDate") {
    statusLabel = "You're on the latest version";
  } else if (updater.status === "available" && updater.available) {
    statusLabel = `Update available: v${updater.available.version}`;
  } else if (updater.status === "downloading") {
    statusLabel =
      pct != null ? `Downloading update… ${pct}%` : "Downloading update…";
  } else if (updater.status === "ready") {
    statusLabel = "Update installed — restarting…";
  } else if (updater.status === "error") {
    statusLabel = updater.error ?? "Update check failed";
  }

  return (
    <>
      <SettingsBlock title="Updates">
        <div className="settings-about-hero">
          <strong>LUMORA</strong>
          <span>v{appVersion}</span>
        </div>
        <p className="muted settings-note">{statusLabel}</p>
        {updater.available?.body ? (
          <pre className="settings-update-notes">{updater.available.body}</pre>
        ) : null}
        <div className="settings-inline-actions">
          <button
            type="button"
            className="primary"
            disabled={busy || updater.isDev}
            onClick={() => void updater.checkForUpdates()}
          >
            {updater.status === "checking" ? "Checking…" : "Check for updates"}
          </button>
          {updater.status === "available" ? (
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => void updater.downloadAndInstall()}
            >
              Download & install
            </button>
          ) : null}
        </div>
      </SettingsBlock>
      {u ? (
        <SettingsBlock title="Preferences">
          <ToggleRow
            label="Check automatically"
            description="Look for a new release when LUMORA starts."
            checked={u.checkAutomatically}
            onChange={(v) =>
              void update((p) => {
                p.updates.checkAutomatically = v;
                return p;
              })
            }
          />
          <ToggleRow
            label="Download in background"
            description="When an update is found automatically, download and install without asking. The app restarts after install."
            checked={u.downloadInBackground}
            disabled={!u.checkAutomatically}
            onChange={(v) =>
              void update((p) => {
                p.updates.downloadInBackground = v;
                return p;
              })
            }
          />
        </SettingsBlock>
      ) : null}
    </>
  );
}

function AboutSection({ appVersion }: { appVersion: string }) {
  return (
    <SettingsBlock title="About">
      <div className="settings-about-hero">
        <strong>LUMORA</strong>
        <span className="muted">your memories your machine.</span>
        <span>Version {appVersion}</span>
      </div>
      <dl className="settings-storage-list">
        <div>
          <dt>License</dt>
          <dd>See repository LICENSE</dd>
        </div>
        <div>
          <dt>Privacy</dt>
          <dd>Everything stays on your machine</dd>
        </div>
        <div>
          <dt>Network</dt>
          <dd>Model downloads, and optional update checks against GitHub Releases</dd>
        </div>
      </dl>
    </SettingsBlock>
  );
}

async function openExternal(url: string) {
  try {
    await openUrl(url);
  } catch (e) {
    console.error("Failed to open URL", url, e);
  }
}

function FromMakerSection() {
  return (
    <>
      <SettingsBlock title="A note from the maker">
        <div className="settings-maker-hero">
          <strong>{MAKER.name}</strong>
          <span className="muted">@{MAKER.githubUser}</span>
        </div>
        <div className="settings-maker-copy">
          <p>
            LUMORA exists because I wanted a photo library that treats an archive like
            something private — not a funnel. Your originals stay on your disk. Search,
            faces, and tags only run when you ask, and they run on your machine.
          </p>
          <p>
            The long-term vision is simple: a calm, local-first home for decades of
            photos and video — fast at scale, honest about what it does online, and open
            enough that you can inspect, fork, and keep it forever.
          </p>
          <p>
            If the app helps you keep memories without renting them back from the cloud,
            a Ko-fi tip keeps the late nights of indexing, model wiring, and release
            signing going. Thank you for being here.
          </p>
        </div>
      </SettingsBlock>

      <SettingsBlock title="Support the work">
        <p className="muted settings-note">
          Optional — LUMORA stays free and open source either way. Support never unlocks
          features or changes privacy defaults.
        </p>
        <div className="settings-maker-actions">
          <button
            type="button"
            className="primary settings-coffee-btn"
            onClick={() => void openExternal(MAKER.koFiUrl)}
          >
            Support on Ko-fi
          </button>
          <button
            type="button"
            onClick={() => void openExternal(MAKER.repoUrl)}
          >
            View on GitHub
          </button>
          <button
            type="button"
            onClick={() => void openExternal(MAKER.githubProfileUrl)}
          >
            Maker profile
          </button>
        </div>
      </SettingsBlock>
    </>
  );
}
