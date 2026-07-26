import { labelHex } from "../../lib/labels";
import type { AssetSummary } from "../../lib/tauri";

/** Bottom-left overlay: colour label dot, rating, and trash countdown. */
export function CardMarks({
  asset,
  trashDays,
}: {
  asset: AssetSummary;
  trashDays: number | null;
}) {
  const hex = labelHex(asset.colorLabel);
  if (!hex && asset.rating <= 0 && trashDays === null) return null;
  return (
    <span className="card-marks">
      {hex && (
        <span
          className="label-dot"
          style={{ background: hex }}
          title={`Label: ${asset.colorLabel}`}
        />
      )}
      {asset.rating > 0 && (
        <span className="rating-chip">★{asset.rating}</span>
      )}
      {trashDays !== null && (
        <span className={`trash-chip ${trashDays <= 3 ? "soon" : ""}`}>
          {trashDays <= 0 ? "removing soon" : `${trashDays}d left`}
        </span>
      )}
    </span>
  );
}
