import { useEffect, useState } from "react";
import { Icon } from "../../components/icons";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { formatDuration, formatMediaDate } from "../../lib/format";
import { labelHex } from "../../lib/labels";
import {
  api,
  fileSrc,
  thumbSrc,
  type AssetLabel,
  type AssetOrganisation,
  type AssetSummary,
  type AssetText,
  type AssetCaption,
  type FaceBox,
} from "../../lib/tauri";

export function MediaInfoPanel({
  asset,
  onClose,
}: {
  asset: AssetSummary;
  onClose: () => void;
}) {
  const [organisation, setOrganisation] = useState<AssetOrganisation | null>(
    null,
  );
  const [loadingOrg, setLoadingOrg] = useState(true);
  const [orgError, setOrgError] = useState<string | null>(null);
  const [assetText, setAssetText] = useState<AssetText | null>(null);
  const [loadingText, setLoadingText] = useState(true);
  const [assetCaption, setAssetCaption] = useState<AssetCaption | null>(null);
  const [loadingCaption, setLoadingCaption] = useState(true);
  const [faces, setFaces] = useState<FaceBox[]>([]);
  const [loadingFaces, setLoadingFaces] = useState(true);
  const [autoTags, setAutoTags] = useState<AssetLabel[]>([]);
  const [loadingAutoTags, setLoadingAutoTags] = useState(true);

  const name = asset.path.split(/[/\\]/).pop() ?? asset.path;
  const folder = asset.path.replace(/[/\\][^/\\]+$/, "") || asset.path;
  const dimensions =
    asset.width && asset.height
      ? `${asset.width.toLocaleString()} × ${asset.height.toLocaleString()}`
      : null;
  const preview = thumbSrc(asset);
  const labelColor = labelHex(asset.colorLabel);

  useEffect(() => {
    let cancelled = false;
    setLoadingOrg(true);
    setOrgError(null);
    void api
      .getAssetOrganisation(asset.id)
      .then((data) => {
        if (!cancelled) setOrganisation(data);
      })
      .catch((error) => {
        if (!cancelled) {
          setOrganisation({ albums: [], tags: [] });
          setOrgError(String(error));
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingOrg(false);
      });
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  useEffect(() => {
    let cancelled = false;
    setLoadingCaption(true);
    void api
      .getAssetCaption(asset.id)
      .then((data) => {
        if (!cancelled) setAssetCaption(data);
      })
      .catch(() => {
        if (!cancelled) setAssetCaption(null);
      })
      .finally(() => {
        if (!cancelled) setLoadingCaption(false);
      });
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  useEffect(() => {
    let cancelled = false;
    setLoadingText(true);
    void api
      .getAssetText(asset.id)
      .then((data) => {
        if (!cancelled) setAssetText(data);
      })
      .catch(() => {
        if (!cancelled) setAssetText(null);
      })
      .finally(() => {
        if (!cancelled) setLoadingText(false);
      });
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  useEffect(() => {
    let cancelled = false;
    setLoadingFaces(true);
    void api
      .listAssetFaces(asset.id)
      .then((data) => {
        if (!cancelled) setFaces(data);
      })
      .catch(() => {
        if (!cancelled) setFaces([]);
      })
      .finally(() => {
        if (!cancelled) setLoadingFaces(false);
      });
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  useEffect(() => {
    let cancelled = false;
    setLoadingAutoTags(true);
    void api
      .listAssetLabels(asset.id)
      .then((data) => {
        if (!cancelled) setAutoTags(data);
      })
      .catch(() => {
        if (!cancelled) setAutoTags([]);
      })
      .finally(() => {
        if (!cancelled) setLoadingAutoTags(false);
      });
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  async function detachFace(faceId: string) {
    try {
      await api.detachFace(faceId);
      setFaces(await api.listAssetFaces(asset.id));
    } catch {
      // Keep existing chips; detach is best-effort from the info panel.
    }
  }

  async function setFaceIgnored(personId: string, ignored: boolean) {
    try {
      await api.setPersonIgnored(personId, ignored);
      setFaces(await api.listAssetFaces(asset.id));
    } catch {
      // Keep existing chips; ignoring is best-effort from the info panel.
    }
  }

  return (
    <>
      <div className="media-info-scrim" onClick={onClose} aria-hidden="true" />
      <aside
        className="media-info-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="media-info-title"
      >
        <header className="media-info-header">
          <div>
            <span className="media-info-eyebrow">Details</span>
            <h2 id="media-info-title" title={name}>
              {name}
            </h2>
          </div>
          <button
            type="button"
            className="media-info-close"
            aria-label="Close media information"
            onClick={onClose}
            autoFocus
          >
            <Icon name="close" />
          </button>
        </header>

        <div className="media-info-body">
          <div className="media-info-hero">
            <div className="media-info-preview">
              <SafeImage
                src={preview}
                alt=""
                loading="lazy"
                fallback={
                  <MediaFallback
                    type={asset.mediaType === "video" ? "video" : "image"}
                  />
                }
              />
            </div>
            <div className="media-info-status">
              <span className="media-info-pill">
                {asset.mediaType === "video" ? "Video" : "Photo"}
              </span>
              {asset.favorite && (
                <span className="media-info-pill fav">
                  <Icon name="heart" />
                  Favourite
                </span>
              )}
              {asset.rating > 0 && (
                <span className="media-info-pill rating">
                  {"★".repeat(asset.rating)}
                </span>
              )}
              {asset.colorLabel && (
                <span className="media-info-pill label">
                  <span
                    className="label-dot"
                    style={{ background: labelColor ?? undefined }}
                  />
                  {asset.colorLabel}
                </span>
              )}
              {asset.deletedAt && (
                <span className="media-info-pill trash">In trash</span>
              )}
            </div>
          </div>

          <section className="media-info-section">
            <h3>Overview</h3>
            <div className="media-info-kv">
              <div className="media-info-row">
                <span>Dimensions</span>
                <strong>
                  {dimensions ? `${dimensions} px` : "Not available"}
                </strong>
              </div>
              {asset.mediaType === "video" && (
                <div className="media-info-row">
                  <span>Duration</span>
                  <strong>{formatDuration(asset.durationMs)}</strong>
                </div>
              )}
              <div className="media-info-row">
                <span>Captured</span>
                <strong>{formatMediaDate(asset.capturedAt)}</strong>
              </div>
              <div className="media-info-row">
                <span>Added</span>
                <strong>{formatMediaDate(asset.createdAt)}</strong>
              </div>
              <div className="media-info-row">
                <span>Indexed</span>
                <strong>{formatMediaDate(asset.indexedAt)}</strong>
              </div>
            </div>
          </section>

          <section className="media-info-section">
            <h3>Camera</h3>
            <div className="media-info-kv">
              <div className="media-info-row">
                <span>Model</span>
                <strong>{asset.camera || "Not available"}</strong>
              </div>
              <div className="media-info-row">
                <span>Lens</span>
                <strong>{asset.lens || "Not available"}</strong>
              </div>
            </div>
          </section>

          <section className="media-info-section">
            <h3>File</h3>
            <div className="media-info-kv">
              <div className="media-info-row stacked">
                <span>Folder</span>
                <strong className="media-info-mono" title={folder}>
                  {folder}
                </strong>
              </div>
              <div className="media-info-row stacked">
                <span>Full path</span>
                <strong className="media-info-mono" title={asset.path}>
                  {asset.path}
                </strong>
              </div>
            </div>
          </section>

          <section className="media-info-section">
            <h3>Organisation</h3>
            <div className="media-info-block">
              <div className="media-info-block-head">
                <Icon name="album" />
                <div>
                  <h4>Albums</h4>
                  <p>
                    {loadingOrg
                      ? "Loading…"
                      : organisation
                        ? `${organisation.albums.length} album${
                            organisation.albums.length === 1 ? "" : "s"
                          }`
                        : "—"}
                  </p>
                </div>
              </div>
              {loadingOrg ? (
                <p className="muted media-info-empty">Fetching membership…</p>
              ) : organisation && organisation.albums.length > 0 ? (
                <ul className="media-info-chips">
                  {organisation.albums.map((album) => (
                    <li key={album.id}>
                      <span className="media-info-chip album">
                        <Icon name="album" />
                        <span>
                          {album.name}
                          <em>{album.assetCount} items</em>
                        </span>
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="muted media-info-empty">
                  Not part of any album
                </p>
              )}
            </div>

            <div className="media-info-block">
              <div className="media-info-block-head">
                <Icon name="label" />
                <div>
                  <h4>Tags</h4>
                  <p>
                    {loadingOrg
                      ? "Loading…"
                      : organisation
                        ? `${organisation.tags.length} tag${
                            organisation.tags.length === 1 ? "" : "s"
                          }`
                        : "—"}
                  </p>
                </div>
              </div>
              {loadingOrg ? (
                <p className="muted media-info-empty">Fetching tags…</p>
              ) : organisation && organisation.tags.length > 0 ? (
                <ul className="media-info-chips">
                  {organisation.tags.map((tag) => (
                    <li key={tag.id}>
                      <span className="media-info-chip tag">
                        <Icon name="label" />
                        {tag.name}
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="muted media-info-empty">No tags assigned</p>
              )}
            </div>

            <div className="media-info-block">
              <div className="media-info-block-head">
                <Icon name="sparkle" />
                <div>
                  <h4>Auto-tags</h4>
                  <p>
                    {loadingAutoTags
                      ? "Loading…"
                      : `${autoTags.length} label${
                          autoTags.length === 1 ? "" : "s"
                        }`}
                  </p>
                </div>
              </div>
              {loadingAutoTags ? (
                <p className="muted media-info-empty">Fetching auto-tags…</p>
              ) : autoTags.length > 0 ? (
                <ul className="media-info-chips">
                  {autoTags.map((tag) => (
                    <li key={`${tag.modelId}-${tag.rank}-${tag.label}`}>
                      <span className="media-info-chip tag" title={`${Math.round(tag.score * 100)}%`}>
                        <Icon name="sparkle" />
                        {tag.label}
                        <em>{Math.round(tag.score * 100)}%</em>
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="muted media-info-empty">
                  No auto-tags yet. Enable object detection and install the
                  MobileNet model in Settings → AI.
                </p>
              )}
            </div>
            {orgError && <p className="muted media-info-empty">{orgError}</p>}
          </section>

          <section className="media-info-section">
            <h3>People in this photo</h3>
            {loadingFaces ? (
              <p className="muted media-info-empty">Checking faces…</p>
            ) : faces.length > 0 ? (
              <ul className="media-info-face-chips">
                {faces.map((face) => {
                  const crop = face.cropPath ? fileSrc(face.cropPath) : null;
                  return (
                    <li key={face.id} className="media-info-face-chip">
                      <SafeImage
                        src={crop}
                        alt=""
                        fallback={<MediaFallback type="album" />}
                      />
                      <span className="muted">
                        {face.personIgnored
                          ? "Ignored"
                          : face.personName?.trim() || "Unnamed"}
                      </span>
                      {face.personIgnored ? (
                        <button
                          type="button"
                          onClick={() =>
                            face.personId &&
                            void setFaceIgnored(face.personId, false)
                          }
                          title="Show this person in People and search again"
                        >
                          Stop ignoring
                        </button>
                      ) : (
                        <>
                          <button
                            type="button"
                            onClick={() => void detachFace(face.id)}
                            title="Split this face into its own person"
                          >
                            Not this person
                          </button>
                          <button
                            type="button"
                            onClick={() =>
                              face.personId &&
                              void setFaceIgnored(face.personId, true)
                            }
                            title="Hide this face and skip it in future imports"
                          >
                            Ignore
                          </button>
                        </>
                      )}
                    </li>
                  );
                })}
              </ul>
            ) : (
              <p className="muted media-info-empty">
                No faces detected yet. Install face models in Settings to enable
                on-device recognition.
              </p>
            )}
          </section>

          <section className="media-info-section">
            <h3>Extracted text</h3>
            {loadingText ? (
              <p className="muted media-info-empty">Checking OCR…</p>
            ) : assetText?.text?.trim() ? (
              <div className="media-info-kv">
                <div className="media-info-row stacked">
                  <span>
                    OCR
                    {assetText.confidence > 0
                      ? ` · ${Math.round(assetText.confidence * 100)}% confidence`
                      : ""}
                  </span>
                  <strong className="media-info-ocr-text">{assetText.text}</strong>
                </div>
              </div>
            ) : (
              <p className="muted media-info-empty">
                No text extracted yet. Install OCR models in Settings to enable
                on-device recognition.
              </p>
            )}
          </section>

          <section className="media-info-section">
            <h3>Image caption</h3>
            {loadingCaption ? (
              <p className="muted media-info-empty">Checking captions…</p>
            ) : assetCaption?.caption?.trim() ? (
              <div className="media-info-kv">
                <div className="media-info-row stacked">
                  <span>Florence-2</span>
                  <strong className="media-info-ocr-text">{assetCaption.caption}</strong>
                </div>
              </div>
            ) : (
              <p className="muted media-info-empty">
                No caption yet. Install Florence-2 and enable image captions in Settings → AI.
              </p>
            )}
          </section>

          <section className="media-info-section">
            <h3>Integrity</h3>
            <div className="media-info-kv">
              <div className="media-info-row stacked">
                <span>SHA-256</span>
                <strong className="media-info-mono">{asset.hash}</strong>
              </div>
              {asset.perceptualHash && (
                <div className="media-info-row stacked">
                  <span>Perceptual hash</span>
                  <strong className="media-info-mono">
                    {asset.perceptualHash}
                  </strong>
                </div>
              )}
              {asset.deletedAt && (
                <div className="media-info-row">
                  <span>Moved to trash</span>
                  <strong>{formatMediaDate(asset.deletedAt)}</strong>
                </div>
              )}
            </div>
          </section>
        </div>
      </aside>
    </>
  );
}
