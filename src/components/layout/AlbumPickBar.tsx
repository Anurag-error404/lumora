import { Icon } from "../icons";
import type { AlbumPickTarget } from "../../types/app";

/** Banner shown while picking library photos to add to an album. */
export function AlbumPickBar({
  target,
  selectedCount,
  onCancel,
  onConfirm,
}: {
  target: AlbumPickTarget;
  selectedCount: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="pick-bar" role="status">
      <Icon name="album" className="pick-bar-icon" />
      <span>
        Adding to <strong>{target.name}</strong>
        {selectedCount > 0
          ? ` · ${selectedCount} selected`
          : " · select photos below"}
      </span>
      <div className="spacer" />
      <button onClick={onCancel}>Cancel</button>
      <button
        className="primary"
        disabled={selectedCount === 0}
        onClick={onConfirm}
      >
        Add to album
      </button>
    </div>
  );
}
