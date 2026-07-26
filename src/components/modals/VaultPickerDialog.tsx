import { useState } from "react";
import { Icon } from "../icons";
import type { VaultSummary } from "../../lib/tauri";

/**
 * Modal to choose which vault receives locked items. If the chosen vault is
 * locked, prompts for its password before confirming.
 */
export function VaultPickerDialog({
  vaults,
  title,
  busy,
  onCancel,
  onConfirm,
}: {
  vaults: VaultSummary[];
  title: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (vaultId: string, password?: string) => Promise<void>;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(
    vaults.find((v) => v.unlocked)?.id ?? vaults[0]?.id ?? null,
  );
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  const selected = vaults.find((v) => v.id === selectedId) ?? null;
  const needsPassword = selected != null && !selected.unlocked;

  async function submit() {
    if (!selected) return;
    if (needsPassword && !password) {
      setError("Enter the password for this vault");
      return;
    }
    setError(null);
    try {
      await onConfirm(selected.id, needsPassword ? password : undefined);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="vault-picker-title">
      <div className="modal vault-picker-dialog" onClick={(e) => e.stopPropagation()}>
        <h2 id="vault-picker-title">{title}</h2>
        <p className="muted">Choose which vault to move these items into.</p>
        <div className="vault-picker-list">
          {vaults.map((vault) => (
            <label
              key={vault.id}
              className={`vault-picker-option ${selectedId === vault.id ? "selected" : ""}`}
            >
              <input
                type="radio"
                name="vault-pick"
                checked={selectedId === vault.id}
                onChange={() => {
                  setSelectedId(vault.id);
                  setPassword("");
                  setError(null);
                }}
              />
              <Icon name={vault.unlocked ? "unlock" : "lock"} className="nav-icon" />
              <span>
                <strong>{vault.name}</strong>
                <small>
                  {vault.lockedCount} item{vault.lockedCount === 1 ? "" : "s"}
                  {vault.unlocked ? " · unlocked" : " · locked"}
                </small>
                <small className="vault-path">{vault.path}</small>
              </span>
            </label>
          ))}
        </div>
        {needsPassword && (
          <label className="vault-field">
            <span>Password for “{selected?.name}”</span>
            <input
              type="password"
              value={password}
              autoFocus
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void submit();
              }}
              placeholder="Password"
            />
          </label>
        )}
        {error && <p className="muted" style={{ color: "var(--danger)" }}>{error}</p>}
        <div className="modal-actions">
          <button className="secondary" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button className="primary" onClick={() => void submit()} disabled={busy || !selected}>
            {busy ? "Moving…" : needsPassword ? "Unlock & move" : "Move in"}
          </button>
        </div>
      </div>
    </div>
  );
}
